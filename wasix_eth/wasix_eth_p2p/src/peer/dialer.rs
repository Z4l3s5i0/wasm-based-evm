use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::PeerDiscoveryWriter;
use wasix_eth_storage::read_traits::PeerDiscoveryProvider;
use wasix_eth_types::sync::P2pSession;
use wasix_eth_types::PeerEntry;
use wasix_eth_utils::{debug, error, info};
use crate::peer::peer_registry::PeerRegistry;
use crate::rlpx::RlpxStream;
use crate::rlpx::handshake::Handshake;
use crate::rlpx::message::{RequestPair};
use crate::rlpx::PeerSession;
use crate::discovery::v4::Enode;
use wasix_eth_types::p2p::{StatusMessage, GetBlockHeaders};
use std::collections::HashMap;
use tokio::sync::Mutex;

pub struct PeerDialer {
    registry: Arc<PeerRegistry>,
    write_provider: DatabaseWriteProvider,
    dial_attempts: Arc<Mutex<HashMap<SocketAddr, (u32, bool)>>>,
}

impl PeerDialer {
    pub fn new(registry: Arc<PeerRegistry>, write_provider: DatabaseWriteProvider) -> Self {
        Self {
            registry,
            write_provider,
            dial_attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn dial_enode(&self, enode_str: &str) {
        if let Ok(enode) = enode_str.parse::<Enode>() {
            let addr = SocketAddr::new(enode.ip, enode.tcp_port);
            self.dial_peer(addr);
        } else if let Ok(addr) = enode_str.parse::<SocketAddr>() {
            self.dial_peer(addr);
        }
    }

    pub fn dial_peer(&self, addr: SocketAddr) {
        let dialer = Arc::new(self.clone_internal());
        tokio::spawn(async move {
            dialer.dial_peer_internal(addr).await;
        });
    }

    fn clone_internal(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            write_provider: self.write_provider.clone(),
            dial_attempts: self.dial_attempts.clone(),
        }
    }

    async fn dial_peer_internal(&self, addr: SocketAddr) {
        let (attempts, last_error_was_eof) = {
            let mut guard = self.dial_attempts.lock().await;
            let entry = guard.entry(addr).or_insert((0, false));
            entry.0 += 1;
            (entry.0, entry.1)
        };

        if attempts > 1 {
            let base_delay: u64 = if last_error_was_eof { 5 } else { 2 };
            let delay = Duration::from_secs(base_delay.pow(attempts.min(5) as u32));
            debug!("[P2P Dialer] Retrying dial to {} in {:?} (attempt {}, eof: {})", addr, delay, attempts, last_error_was_eof);
            sleep(delay).await;
        }

        info!("[P2P Dialer] Dialing peer at {} (attempt {})", addr, attempts);

        // Pre-check: Is this address already in our peer pool?
        let peer_pool = if let Ok(pool) = self.registry.read_provider.get_active_peers() {
            pool
        } else {
            return;
        };

        for entry in peer_pool.iter() {
            if entry.discovery_addr == addr {
                debug!("[P2P Dialer] Already bonded to peer at {} (ID: {}), skipping dial", addr, entry.peer_id);
                return;
            }
        }

        let local_sk = self.registry.local_identity().secret_key();
        let remote_pk_res = if let Some(service) = self.registry.get_discovery_service_v4().await {
            service.get_node_by_addr(addr).await.and_then(|node| crate::rlpx::crypto::b512_to_pubkey(&node.id).ok())
        } else {
            None
        };

        match RlpxStream::connect(&addr.to_string(), &local_sk, &alloy_primitives::B512::ZERO).await {
            Ok(stream) => {
                let handshake = Handshake::new(self.registry.clone());
                let remote_pk = if let Some(pk) = remote_pk_res {
                    pk
                } else {
                    error!("[P2P Dialer] Cannot dial {} without remote public key", addr);
                    return;
                };

                match handshake.handle_outbound(stream, &remote_pk).await {
                    Ok((rlpx_stream, remote_status)) => {
                        let remote_id_hex = format!("{:?}", rlpx_stream.remote_id.unwrap());
                        let remote_block_hash = match &remote_status {
                            StatusMessage::Legacy(s) => s.blockhash,
                            StatusMessage::Eth69(s) => s.blockhash,
                        };
                        debug!("[P2P Dialer] Handshake successful with {} (PeerId: {}). Remote head: {:?}", addr, remote_id_hex, remote_block_hash);

                        if peer_pool.iter().any(|e| e.peer_id == remote_id_hex) {
                            debug!("[P2P Dialer] Already bonded to peer {}", remote_id_hex);
                            return;
                        }

                        if let Err(e) = self.write_provider.register_peer(
                            PeerEntry {
                                peer_id: remote_id_hex.clone(),
                                discovery_addr: addr,
                                p2p_addr: addr,
                            }
                        ) {
                            error!("[P2P Dialer] Failed to register peer in DB: {}", e);
                        }

                        let gossip_tx = self.registry.get_gossip_tx().await;
                        let session = Arc::new(PeerSession::new(rlpx_stream, addr, gossip_tx, Some(self.registry.disconnect_tx())));
                        let head_hash = remote_block_hash;
                        {
                            let mut guard = session.status.lock().await;
                            *guard = Some(remote_status);
                        }
                        let session_clone = session.clone();
                        let remote_id_clone = remote_id_hex.clone();
                        tokio::spawn(async move {
                            let request = RequestPair {
                                request_id: 1,
                                message: GetBlockHeaders {
                                    block: wasix_eth_types::p2p::BlockHashOrNumber::Hash(head_hash),
                                    amount: 1,
                                    skip: 0,
                                    reverse: false,
                                },
                            };
                            if let Ok(response) = session_clone.get_block_headers(request).await {
                                if let Some(header) = response.message.0.first() {
                                    let mut h_guard = session_clone.best_height.lock().await;
                                    if header.number > *h_guard {
                                        *h_guard = header.number;
                                        info!("[P2P Dialer] Set initial best_height for peer {} to {}", remote_id_clone, header.number);
                                    }
                                }
                            }
                        });

                        self.registry.register_session_arc(remote_id_hex.clone(), session).await;
                        info!("[P2P Dialer] Successfully bonded with {} (PeerId: {})", addr, remote_id_hex);
                        
                        // Success! Reset attempts
                        self.dial_attempts.lock().await.remove(&addr);
                    }
                    Err(e) => {
                        error!("[P2P Dialer] Handshake failed with {}: {}", addr, e);
                        let is_eof = e.to_string().contains("early eof");
                        if is_eof {
                            let mut guard = self.dial_attempts.lock().await;
                            if let Some(entry) = guard.get_mut(&addr) {
                                entry.1 = true;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("[P2P Dialer] Connection failed with {}: {}", addr, e);
                let is_eof = e.to_string().contains("early eof");
                if is_eof {
                    let mut guard = self.dial_attempts.lock().await;
                    if let Some(entry) = guard.get_mut(&addr) {
                        entry.1 = true;
                    }
                }
            }
        }
    }
}
