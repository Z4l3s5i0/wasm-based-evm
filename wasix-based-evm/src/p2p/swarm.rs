use crate::p2p::connection::PeerInfoRlp;
use crate::p2p::identity::Identity;
use crate::p2p::{P2pApiServer, HelloResponse, P2pApiClient};
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use jsonrpsee::server::ServerBuilder;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClientBuilder;
use crate::{info, error, debug};
use tokio::sync::mpsc;

const MAX_PEERS: usize = 50;

pub struct PeerInfo {
    pub addr: SocketAddr,
    pub rpc_url: String,
}

#[derive(Clone)]
pub struct PeerManager {
    local_identity: Identity,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    gossip_tx: mpsc::Sender<Vec<u8>>,
    pub port: u16,
    pub bootnodes: Vec<String>,
}

#[async_trait::async_trait]
impl P2pApiServer for PeerManager {
    async fn hello(&self, peer_id: String, listen_port: u16) -> RpcResult<HelloResponse> {
        info!("[P2P] Received hello from {} (port: {})", peer_id, listen_port);
        // In the new model, we might not need to keep an mpsc::Sender if we use reqwest for outbound.
        // However, the current PeerInfo struct uses it. For now, we'll keep it as is, 
        // but note that the client implementation will eventually replace how we communicate back.
        
        Ok(HelloResponse {
            peer_id: self.local_identity.peer_id(),
        })
    }

    async fn get_peers(&self) -> RpcResult<Vec<PeerInfoRlp>> {
        let peers_lock = self.peers.read().await;
        let mut list = Vec::new();
        for (id, peer_info) in peers_lock.iter() {
            if let Some(addr) = peer_info.addr {
                list.push(PeerInfoRlp {
                    peer_id: id.clone(),
                    addr: addr.to_string(),
                });
            }
            if list.len() >= 16 {
                break;
            }
        }
        Ok(list)
    }

    async fn gossip(&self, _topic: String, data: Vec<u8>) -> RpcResult<()> {
        debug!("[P2P] Received gossip message ({} bytes)", data.len());
        let _ = self.gossip_tx.send(data).await;
        Ok(())
    }
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
        let server = ServerBuilder::default().build(addr).await?;
        let handle = server.start(self.clone().into_rpc());
        info!("[P2P] RPC Server started on {}", addr);

        // Keep the server running in a separate task
        tokio::spawn(handle.stopped());

        // Start Heartbeat loop
        let peers_hb = self.peers.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                let peers_lock = peers_hb.read().await;
                debug!("[P2P] Sending heartbeats to {} peers", peers_lock.len());
                for (peer_id, info) in peers_lock.iter() {
                    if let Err(_) = info.tx.send(Message::Ping).await {
                        debug!("[P2P] Failed to send Ping to {}", peer_id);
                    }
                }
            }
        });

        // Connect to bootnodes
        let self_cloned = self.clone();
        for bootnode in bootnodes {
            if let Ok(addr) = bootnode.parse::<SocketAddr>() {
                Arc::new(self_cloned.clone()).dial_peer(addr);
            } else {
                error!("[P2P] Invalid bootnode address: {}", bootnode);
            }
        }

        Ok(())
    }

    pub fn dial_peer(self: Arc<Self>, addr: SocketAddr) {
        let (tx, rx) = mpsc::channel(100);
        let local_id = self.local_identity.peer_id();
        let peers_inner = self.peers.clone();
        let listen_port = self.port;
        let pm_inner = self.clone();
        
        tokio::spawn(async move {
            debug!("[P2P] Attempting to connect to peer {}", addr);
            match tokio::time::timeout(tokio::time::Duration::from_secs(30), TcpStream::connect(addr)).await {
                Ok(Ok(stream)) => {
                    let local_addr = stream.local_addr().ok();
                    debug!("[P2P] TCP connection established with {} from local {:?}", addr, local_addr);
                    debug!("[P2P] Starting handshake (No TLS) with {}", addr);
                    let pm_for_conn = pm_inner.clone();
                    match Connection::new_client(stream, local_id, Some(listen_port), rx).await {
                        Ok((conn, remote_id, remote_listen_port)) => {
                            info!("[P2P] Connected to {} with PeerId {}", addr, remote_id);
                            
                            let mut remote_addr = Some(addr);
                            if let Some(port) = remote_listen_port {
                                remote_addr.as_mut().unwrap().set_port(port);
                            }

                            // Register peer
                            {
                                let mut peers_lock = peers_inner.write().await;
                                if peers_lock.contains_key(&remote_id) {
                                    debug!("[P2P] Already connected to {}", remote_id);
                                    return;
                                }
                                if peers_lock.len() >= MAX_PEERS {
                                    info!("[P2P] Max peers reached, dropping discovered {}", remote_id);
                                    return;
                                }
                                peers_lock.insert(remote_id.clone(), PeerInfo { tx, addr: remote_addr });
                            }
                            
                            // Send our peer list to the new peer
                            let list = {
                                let peers_lock = peers_inner.read().await;
                                let mut list = Vec::new();
                                for (id, peer_info) in peers_lock.iter() {
                                    if id != &remote_id {
                                        if let Some(a) = peer_info.addr {
                                            list.push(PeerInfoRlp { peer_id: id.clone(), addr: a.to_string() });
                                        }
                                    }
                                    if list.len() >= 16 { break; }
                                }
                                list
                            };

                            if !list.is_empty() {
                                let peers_lock = peers_inner.read().await;
                                if let Some(info) = peers_lock.get(&remote_id) {
                                    let _ = info.tx.send(Message::PeerList(list)).await;
                                }
                            }

                            if let Err(e) = conn.process(pm_for_conn).await {
                                error!("[P2P] Connection error with {}: {}", remote_id, e);
                            }
                            
                            // Cleanup peer
                            peers_inner.write().await.remove(&remote_id);
                        }
                        Err(e) => error!("[P2P] Handshake failed with {}: {}", addr, e),
                    }
                }
                Ok(Err(e)) => error!("[P2P] Failed to connect to {}: {}", addr, e),
                Err(_) => error!("[P2P] Connection timeout (TCP) with {}", addr),
            }
        });
    }

    pub async fn broadcast_gossip(&self, data: Vec<u8>) {
        let peers = self.peers.read().await;
        for (peer_id, info) in peers.iter() {
            if let Err(e) = info.tx.send(Message::Gossip(data.clone())).await {
                error!("[P2P] Failed to send gossip to {}: {}", peer_id, e);
            }
        }
    }

    pub async fn send_peer_list(&self, peer_id: &str) {
        let list = {
            let peers_lock = self.peers.read().await;
            let mut list = Vec::new();
            for (id, info) in peers_lock.iter() {
                if id != peer_id {
                    if let Some(addr) = info.addr {
                        list.push(PeerInfoRlp { peer_id: id.clone(), addr: addr.to_string() });
                    }
                }
                if list.len() >= 16 {
                    break;
                }
            }
            list
        };

        if !list.is_empty() {
            let peers_lock = self.peers.read().await;
            if let Some(info) = peers_lock.get(peer_id) {
                let _ = info.tx.send(Message::PeerList(list)).await;
            }
        }
    }
}
