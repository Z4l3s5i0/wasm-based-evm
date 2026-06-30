use std::sync::Arc;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use crate::rlpx::handshake::Handshake;
use crate::rlpx::message::{RequestPair, GetBlockHeaders};
use wasix_eth_types::sync::P2pSession;
use crate::rlpx::session::PeerSession;
use wasix_eth_utils::{error, info};
use crate::peer::peer_registry::PeerRegistry;
use wasix_eth_types::p2p::StatusMessage;
use wasix_eth_types::PeerEntry;

pub struct P2pServer {
    listener: TcpListener,
    peer_registry: Arc<PeerRegistry>,
}

impl P2pServer {
    pub async fn new(addr: &str, peer_registry: Arc<PeerRegistry>) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            wasix_eth_utils::error!("[P2P Server] Failed to bind to {}: {}", addr, e);
            e
        })?;
        Ok(Self {
            listener,
            peer_registry,
        })
    }

    pub async fn run(self) {
        let local_addr = self.listener.local_addr().unwrap();
        let port = if local_addr.port() == 0 {
            self.peer_registry.p2p_port()
        } else {
            local_addr.port()
        };

        let mut display_addr = format!("{}:{}", local_addr.ip(), port);
        
        // If it's 0.0.0.0, try to get the external IP from the registry
        if local_addr.ip().is_unspecified() {
            if let Some(ext_ip) = self.peer_registry.ext_ip {
                display_addr = format!("{}:{}", ext_ip, port);
            }
        }
        
        info!("[P2P Server] Listening on {}", display_addr);
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    let registry = self.peer_registry.clone();
                    tokio::spawn(async move {
                        info!("[P2P Server] Handling connection from {}", addr);
                        if let Err(e) = Self::handle_connection(stream, addr, registry).await {
                            error!("[P2P Server] Error handling connection from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("[P2P Server] Accept error: {}", e);
                }
            }
        }
    }

    async fn handle_connection(stream: tokio::net::TcpStream, addr: SocketAddr, registry: Arc<PeerRegistry>) -> anyhow::Result<()> {
        let handshake = Handshake::new(registry.clone());
        let (rlpx_stream, remote_status_msg) = handshake.handle_inbound(stream).await?;
        
        let remote_id = rlpx_stream.remote_id.unwrap();
        let remote_id_hex = format!("{:?}", remote_id);
        
        let head_hash = match &remote_status_msg {
            StatusMessage::Legacy(s) => s.blockhash,
            StatusMessage::Eth69(s) => s.blockhash,
        };
        
        info!("[P2P Server] Handshake successful with peer {}. Remote head: {:?}", remote_id_hex, head_hash);

        // Register peer in database so it's visible to sync
        registry.register_peer(PeerEntry {
            peer_id: remote_id_hex.clone(),
            discovery_addr: addr,
            p2p_addr: addr,
        }).ok();
        
        let gossip_tx = registry.get_gossip_tx().await;
        let session = Arc::new(PeerSession::new(rlpx_stream, addr, gossip_tx, Some(registry.disconnect_tx())));
        {
            let mut guard = session.status.lock().await;
            *guard = Some(remote_status_msg.clone());
        }

        // Request head header to get height
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
                        info!("[P2P] Set initial best_height for peer {} to {}", remote_id_clone, header.number);
                    }
                }
            }
        });

        registry.register_session_arc(remote_id_hex, session).await;
        
        Ok(())
    }
}
