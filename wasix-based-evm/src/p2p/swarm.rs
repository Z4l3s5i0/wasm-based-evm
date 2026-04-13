use crate::{info, error, debug};
use crate::p2p::connection::{Connection, Message};
use crate::p2p::identity::Identity;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};

const MAX_PEERS: usize = 50;

#[derive(Clone)]
pub struct PeerManager {
    local_identity: Identity,
    peers: Arc<RwLock<HashMap<String, mpsc::Sender<Message>>>>,
    gossip_tx: mpsc::Sender<Vec<u8>>,
    pub port: u16,
    pub bootnodes: Vec<String>,
}

impl PeerManager {
    pub fn new(
        identity: Identity,
        port: u16,
        bootnodes: Vec<String>,
    ) -> Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let (gossip_tx, gossip_rx) = mpsc::channel(100);
        Ok((Self {
            local_identity: identity,
            peers: Arc::new(RwLock::new(HashMap::new())),
            gossip_tx,
            port,
            bootnodes,
        }, gossip_rx))
    }

    pub async fn start(
        &self,
        listen_port: u16,
        bootnodes: Vec<String>,
    ) -> Result<()> {
        let addr: SocketAddr = format!("0.0.0.0:{}", listen_port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        info!("[P2P] Listening on {}", addr);

        let peers = self.peers.clone();

        // Start listener loop
        let local_identity = self.local_identity.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("[P2P] Accepted connection from {}", addr);
                        let (tx, rx) = mpsc::channel(100);
                        let local_id = local_identity.peer_id();
                        let peers_inner = peers.clone();
                        
                        tokio::spawn(async move {
                            match Connection::new_server(stream, local_id, rx).await {
                                Ok((conn, remote_id)) => {
                                    info!("[P2P] Handshake successful (No TLS) with {} at {}", remote_id, addr);
                                    
                                    // Register peer
                                    {
                                        let mut peers_lock = peers_inner.write().await;
                                        if peers_lock.len() >= MAX_PEERS {
                                            info!("[P2P] Max peers reached, dropping {}", remote_id);
                                            return;
                                        }
                                        peers_lock.insert(remote_id.clone(), tx);
                                    }
                                    
                                    if let Err(e) = conn.process().await {
                                        error!("[P2P] Connection error with {}: {}", remote_id, e);
                                    }
                                    
                                    // Cleanup peer
                                    peers_inner.write().await.remove(&remote_id);
                                }
                                Err(e) => error!("[P2P] Handshake failed with {}: {}", addr, e),
                            }
                        });
                    }
                    Err(e) => error!("[P2P] Accept error: {}", e),
                }
            }
        });

        // Connect to bootnodes
        for bootnode in bootnodes {
            let addr: SocketAddr = bootnode.parse()?;
            let (tx, rx) = mpsc::channel(100);
            let local_id = self.local_identity.peer_id();
            let peers_inner = self.peers.clone();
            
            tokio::spawn(async move {
                debug!("[P2P] Attempting to connect to bootnode {}", addr);
                match tokio::time::timeout(tokio::time::Duration::from_secs(30), TcpStream::connect(addr)).await {
                    Ok(Ok(stream)) => {
                        let local_addr = stream.local_addr().ok();
                        debug!("[P2P] TCP connection established with bootnode {} from local {:?}", addr, local_addr);
                        debug!("[P2P] Starting handshake (No TLS) with bootnode {}", addr);
                        match Connection::new_client(stream, local_id, rx).await {
                            Ok((conn, remote_id)) => {
                                info!("[P2P] Connected to bootnode {} with PeerId {}", addr, remote_id);
                                
                                // Register peer
                                {
                                    let mut peers_lock = peers_inner.write().await;
                                    peers_lock.insert(remote_id.clone(), tx);
                                }
                                
                                if let Err(e) = conn.process().await {
                                    error!("[P2P] Connection error with bootnode {}: {}", remote_id, e);
                                }
                                
                                // Cleanup peer
                                peers_inner.write().await.remove(&remote_id);
                            }
                            Err(e) => error!("[P2P] Handshake failed with bootnode {}: {}", addr, e),
                        }
                    }
                    Ok(Err(e)) => error!("[P2P] Failed to connect to bootnode {}: {}", addr, e),
                    Err(_) => error!("[P2P] Connection timeout (TCP) with bootnode {}", addr),
                }
            });
        }

        Ok(())
    }

    pub async fn broadcast_gossip(&self, data: Vec<u8>) {
        let peers = self.peers.read().await;
        for (peer_id, tx) in peers.iter() {
            if let Err(e) = tx.send(Message::Gossip(data.clone())).await {
                error!("[P2P] Failed to send gossip to {}: {}", peer_id, e);
            }
        }
    }

    /// Broadcast known peers to a specific peer (PEX)
    pub async fn send_peer_list(&self, peer_id: &str, _listen_port: u16) {
        let peers_to_send = {
            let _peers = self.peers.read().await;
            let list = Vec::new();
            // In a real system, we'd need to know the public address of these peers.
            // For now, we only have their PeerId. We'll skip for this simplified version
            // or send a dummy list if we had actual addresses.
            list
        };

        if !peers_to_send.is_empty() {
            let peers = self.peers.read().await;
            if let Some(tx) = peers.get(peer_id) {
                let _ = tx.send(Message::PeerList(peers_to_send)).await;
            }
        }
    }
}
