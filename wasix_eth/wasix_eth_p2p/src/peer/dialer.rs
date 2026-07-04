use rand::Rng;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{Duration, sleep, timeout};
use std::time::Instant;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::PeerDiscoveryWriter;
use wasix_eth_storage::read_traits::PeerDiscoveryProvider;
use wasix_eth_types::sync::P2pSession;
use wasix_eth_types::PeerEntry;
use wasix_eth_utils::{debug, error, info};
use wasix_eth_utils::metrics::P2P_CONNECTION_ERRORS_TOTAL;
use crate::peer::peer_registry::PeerRegistry;
use crate::rlpx::RlpxStream;
use crate::rlpx::handshake::Handshake;
use crate::rlpx::message::{RequestPair};
use crate::rlpx::PeerSession;
use crate::discovery::v4::Enode;
use wasix_eth_types::p2p::{StatusMessage, GetBlockHeaders};
use std::collections::{HashMap, HashSet};
use tokio::sync::{Mutex, Semaphore};

pub struct PeerDialer {
    registry: Arc<PeerRegistry>,
    write_provider: DatabaseWriteProvider,
    dial_attempts: Arc<Mutex<HashMap<SocketAddr, (u32, Instant)>>>,
    protocol_penalties: Arc<Mutex<HashMap<SocketAddr, Instant>>>,
    in_flight: Arc<Mutex<HashSet<SocketAddr>>>,
    dial_limit: Arc<Semaphore>,
}

impl PeerDialer {
    pub fn new(registry: Arc<PeerRegistry>, write_provider: DatabaseWriteProvider) -> Self {
        Self {
            registry,
            write_provider,
            dial_attempts: Arc::new(Mutex::new(HashMap::new())),
            protocol_penalties: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            dial_limit: Arc::new(Semaphore::new(10)),
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
        // Fast path check BEFORE spawning and BEFORE semaphore
        {
            if let Ok(in_flight) = self.in_flight.try_lock() {
                if in_flight.contains(&addr) {
                    return;
                }
            }
        }

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
            protocol_penalties: self.protocol_penalties.clone(),
            in_flight: self.in_flight.clone(),
            dial_limit: self.dial_limit.clone(),
        }
    }

    async fn dial_peer_internal(&self, addr: SocketAddr) {
        {
            let mut in_flight = self.in_flight.lock().await;
            if !in_flight.insert(addr) {
                debug!("[P2P Dialer] Dial to {} already in progress, skipping duplicate", addr);
                return;
            }
        }

        let permit = match tokio::time::timeout(Duration::from_secs(20), self.dial_limit.clone().acquire_owned()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                error!("[P2P Dialer] Dial semaphore closed, skipping {}", addr);
                let mut in_flight = self.in_flight.lock().await;
                in_flight.remove(&addr);
                return;
            }
            Err(_) => {
                debug!("[P2P Dialer] Timed out waiting for dial slot to {}", addr);
                let mut in_flight = self.in_flight.lock().await;
                in_flight.remove(&addr);
                return;
            }
        };

        let result = self.dial_peer_internal_guarded(addr).await;

        {
            let mut in_flight = self.in_flight.lock().await;
            in_flight.remove(&addr);
        }

        drop(permit);

        if let Err(e) = result {
            error!("[P2P Dialer] Dial task failed for {}: {}", addr, e);
        }
    }

    async fn dial_peer_internal_guarded(&self, addr: SocketAddr) -> anyhow::Result<()> {
        let (attempts, last_dial_time) = {
            let mut guard = self.dial_attempts.lock().await;
            let entry = guard.entry(addr).or_insert((0, Instant::now() - Duration::from_secs(60)));
            let res = (entry.0, entry.1);
            
            // Increment attempts and update time immediately to avoid separate lock
            entry.0 += 1;
            entry.1 = Instant::now();
            res
        };

        if attempts > 0 {
            let elapsed = Instant::now().duration_since(last_dial_time);
            let jitter = rand::thread_rng().gen_range(0..10);
            let cooldown = Duration::from_secs(5 + jitter);
            if elapsed < cooldown {
                let wait = cooldown - elapsed;
                debug!("[P2P Dialer] Cooldown for {}: waiting {:?} (jittered)", addr, wait);
                sleep(wait).await;
            }
        }

        {
            let penalties = self.protocol_penalties.lock().await;
            if let Some(penalty_until) = penalties.get(&addr) {
                if Instant::now() < *penalty_until {
                    debug!("[P2P Dialer] Peer {} is penalized, skipping dial", addr);
                    return Ok(());
                }
            }
        }

        info!("[P2P Dialer] Dialing peer at {} (attempt {})", addr, attempts + 1);

        // Pre-check: Is this address already in our peer pool?
        let peer_pool = if let Ok(pool) = self.registry.read_provider.get_active_peers() {
            pool
        } else {
            return Ok(());
        };

        for entry in peer_pool.iter() {
            if entry.discovery_addr == addr {
                debug!("[P2P Dialer] Already bonded to peer at {} (ID: {}), skipping dial", addr, entry.peer_id);
                return Ok(());
            }
        }

        let local_sk = self.registry.local_identity().secret_key();
        let remote_pk_res = if let Some(service) = self.registry.get_discovery_service_v4().await {
            service.get_node_by_addr(addr).await.and_then(|node| crate::rlpx::crypto::b512_to_pubkey(&node.id).ok())
        } else {
            None
        };

        let connect_result = timeout(
            Duration::from_secs(5),
            RlpxStream::connect(&addr.to_string(), &local_sk, &alloy_primitives::B512::ZERO),
        )
        .await;

        match connect_result {
            Ok(Ok(rlpx_stream)) => {
                info!("[P2P Dialer] TCP connection established to {}", addr);

                let handshake = Handshake::new(self.registry.clone());
                let remote_pk = if let Some(pk) = remote_pk_res {
                    pk
                } else {
                    error!("[P2P Dialer] Cannot dial {} without remote public key", addr);
                    P2P_CONNECTION_ERRORS_TOTAL.inc();
                    return Ok(());
                };

                match timeout(
                    Duration::from_secs(30),
                    handshake.handle_outbound(rlpx_stream, &remote_pk),
                ).await {
                    Ok(Ok((rlpx_stream, remote_status))) => {
                        let remote_id_hex = format!("{:?}", rlpx_stream.remote_id.unwrap());
                        let remote_block_hash = match &remote_status {
                            StatusMessage::Legacy(s) => s.blockhash,
                            StatusMessage::Eth69(s) => s.blockhash,
                        };
                        debug!("[P2P Dialer] Handshake successful with {} (PeerId: {}). Remote head: {:?}", addr, remote_id_hex, remote_block_hash);

                        tokio::task::yield_now().await;

                        if peer_pool.iter().any(|e| e.peer_id == remote_id_hex) {
                            debug!("[P2P Dialer] Already bonded to peer {}", remote_id_hex);
                            return Ok(());
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
                        let session_id = self.registry.next_session_id();
                        let (session, task) = PeerSession::new(rlpx_stream, addr, session_id, gossip_tx, Some(self.registry.disconnect_tx()));
                        let session = Arc::new(session);
                        let head_hash = remote_block_hash;
                        {
                            let mut guard = session.status.lock().await;
                            *guard = Some(remote_status);
                        }
                        
                        // Register BEFORE spawning the task to avoid race
                        self.registry.register_session_arc(remote_id_hex.clone(), session_id, session.clone()).await;

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
                            if let Ok(Ok(response)) = timeout(
                                Duration::from_secs(5),
                                session_clone.get_block_headers(request),
                            ).await {
                                if let Some(header) = response.message.0.first() {
                                    let mut h_guard = session_clone.best_height.lock().await;
                                    if header.number > *h_guard {
                                        *h_guard = header.number;
                                        info!("[P2P Dialer] Set initial best_height for peer {} to {}", remote_id_clone, header.number);
                                    }
                                }
                            }
                        });

                        tokio::spawn(task.run());
                        // Success! Reset attempts
                        self.dial_attempts.lock().await.remove(&addr);
                    }
                    Ok(Err(e)) => {
                        error!("[P2P Dialer] Handshake failed with {}: {}", addr, e);
                        // P2P_CONNECTION_ERRORS_TOTAL.inc();
                        let err_str = e.to_string();

                        if err_str.contains("MAC mismatch") {
                            info!("[P2P Dialer] Protocol error (MAC mismatch) with {}, penalizing", addr);
                            let mut penalties = self.protocol_penalties.lock().await;
                            penalties.insert(addr, Instant::now() + Duration::from_secs(60));
                        }
                    }
                    Err(_) => {
                        error!("[P2P Dialer] Handshake timed out with {}", addr);
                        P2P_CONNECTION_ERRORS_TOTAL.inc();
                    }
                }
            }
            Ok(Err(e)) => {
                error!("[P2P Dialer] Connection failed with {}: {}", addr, e);
                P2P_CONNECTION_ERRORS_TOTAL.inc();
            }
            Err(_) => {
                error!("[P2P Dialer] TCP connect timed out with {}", addr);
                P2P_CONNECTION_ERRORS_TOTAL.inc();
            }
        }

        Ok(())
    }
}
