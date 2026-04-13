use crate::{info, error};
use crate::p2p::connection::{Connection, Message};
use crate::p2p::identity::Identity;
use anyhow::Result;
use tokio_rustls::rustls::{ClientConfig, ServerConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};

#[derive(Clone)]
pub struct PeerManager {
    local_identity: Identity,
    peers: Arc<RwLock<HashMap<String, mpsc::Sender<Message>>>>,
    server_config: Arc<ServerConfig>,
    client_config: Arc<ClientConfig>,
    gossip_tx: mpsc::Sender<Vec<u8>>,
    pub port: u16,
    pub bootnodes: Vec<String>,
}

impl PeerManager {
    pub fn new(
        identity: Identity,
        server_config: ServerConfig,
        client_config: ClientConfig,
        port: u16,
        bootnodes: Vec<String>,
    ) -> Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let (gossip_tx, gossip_rx) = mpsc::channel(100);
        Ok((Self {
            local_identity: identity,
            peers: Arc::new(RwLock::new(HashMap::new())),
            server_config: Arc::new(server_config),
            client_config: Arc::new(client_config),
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
        let server_config = self.server_config.clone();
        let client_config = self.client_config.clone();

        // Start listener loop
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("[P2P] Accepted connection from {}", addr);
                        let (_tx, rx) = mpsc::channel(100);
                        let config = server_config.clone();
                        
                        tokio::spawn(async move {
                            if let Ok(conn) = Connection::new_server(stream, config, rx).await {
                                // For now, we don't have the peer_id yet, but we'll add it in the handshake
                                // This is a placeholder
                                if let Err(e) = conn.process().await {
                                    error!("[P2P] Connection error with {}: {}", addr, e);
                                }
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
            let config = client_config.clone();
            let (_tx, rx) = mpsc::channel(100);
            
            tokio::spawn(async move {
                match TcpStream::connect(addr).await {
                    Ok(stream) => {
                        // In a real system, we'd need the server name for TLS SNI
                        let server_name = "localhost".try_into().unwrap(); 
                        if let Ok(conn) = Connection::new_client(stream, config, server_name, rx).await {
                            if let Err(e) = conn.process().await {
                                error!("[P2P] Connection error with bootnode {}: {}", addr, e);
                            }
                        }
                    }
                    Err(e) => error!("[P2P] Failed to connect to bootnode {}: {}", addr, e),
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
}
