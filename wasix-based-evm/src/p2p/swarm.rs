use crate::p2p::{P2pApiServer, HelloResponse, P2pApiClient, PeerInfoRlp};
use crate::p2p::rpc_client::RpcClient;
use crate::p2p::identity::Identity;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use jsonrpsee::server::ServerBuilder;
use jsonrpsee::core::RpcResult;
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
        Ok(HelloResponse {
            peer_id: self.local_identity.peer_id(),
        })
    }

    async fn ping(&self) -> RpcResult<String> {
        Ok("pong".to_string())
    }

    async fn get_peers(&self) -> RpcResult<Vec<PeerInfoRlp>> {
        let peers_lock = self.peers.read().await;
        let mut list = Vec::new();
        for (id, peer_info) in peers_lock.iter() {
            list.push(PeerInfoRlp {
                peer_id: id.clone(),
                addr: peer_info.addr.to_string(),
            });
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

        // Connect to bootnodes
        let self_cloned = self.clone();
        for bootnode in bootnodes {
            if let Ok(addr) = bootnode.parse::<SocketAddr>() {
                Arc::new(self_cloned.clone()).dial_peer(addr);
            } else {
                error!("[P2P] Invalid bootnode address: {}", bootnode);
            }
        }

        // Start Discovery & Cleanup loop
        let pm = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                pm.discover_peers().await;
                pm.cleanup_stale_peers().await;
            }
        });

        Ok(())
    }

    async fn discover_peers(&self) {
        debug!("[P2P] Starting peer discovery cycle...");
        let peers_lock = self.peers.read().await;
        let peer_list: Vec<(String, String)> = peers_lock.iter()
            .map(|(id, info)| (id.clone(), info.rpc_url.clone()))
            .collect();
        drop(peers_lock);
        
        if peer_list.is_empty() {
             debug!("[P2P] No peers currently known, cannot discover more.");
             return;
        }

        for (id, url) in peer_list {
            let client = RpcClient::new(url.clone());
            debug!("[P2P] Requesting peer list from {} ({})", id, url);
            match client.get_peers().await {
                Ok(new_peers) => {
                    debug!("[P2P] Received {} peers from {}", new_peers.len(), id);
                    for p in new_peers {
                        if let Ok(addr) = p.addr.parse::<SocketAddr>() {
                            if p.peer_id != self.local_identity.peer_id() {
                                let pm = self.clone();
                                tokio::spawn(async move {
                                    Arc::new(pm).dial_peer(addr);
                                });
                            }
                        }
                    }
                }
                Err(e) => debug!("[P2P] Failed to get peers from {}: {}", id, e),
            }
        }
    }

    async fn cleanup_stale_peers(&self) {
        debug!("[P2P] Starting health check / cleanup cycle...");
        let peers_lock = self.peers.read().await;
        let peer_list: Vec<(String, String)> = peers_lock.iter()
            .map(|(id, info)| (id.clone(), info.rpc_url.clone()))
            .collect();
        drop(peers_lock);

        let mut to_remove = Vec::new();
        for (id, url) in peer_list {
            let client = RpcClient::new(url.clone());
            debug!("[P2P] Pinging peer {} ({}) to check health", id, url);
            match client.ping().await {
                Ok(res) => {
                    if res == "pong" {
                        debug!("[P2P] Peer {} is active (received pong)", id);
                    } else {
                        info!("[P2P] Peer {} returned unexpected response: {}", id, res);
                        to_remove.push(id);
                    }
                }
                Err(e) => {
                    info!("[P2P] Peer {} is unreachable: {}. Removing from active pool.", id, e);
                    to_remove.push(id);
                }
            }
        }

        if !to_remove.is_empty() {
            let mut peers_lock = self.peers.write().await;
            for id in to_remove {
                peers_lock.remove(&id);
                info!("[P2P] Successfully removed stale peer {}", id);
            }
            info!("[P2P] Current active peer count: {}", peers_lock.len());
        } else {
            debug!("[P2P] All peers are healthy.");
        }
    }

    pub fn dial_peer(self: Arc<Self>, addr: SocketAddr) {
        let local_id = self.local_identity.peer_id();
        let peers_inner = self.peers.clone();
        let listen_port = self.port;
        
        tokio::spawn(async move {
            info!("[P2P] Attempting to bond with peer at {}", addr);
            let rpc_url = format!("http://{}", addr);
            
            let client = RpcClient::new(rpc_url.clone());
            match client.hello(local_id, listen_port).await {
                Ok(resp) => {
                    let remote_id = resp.peer_id;
                    info!("[P2P] Bonding successful with {} (PeerId: {})", addr, remote_id);
                    
                    let mut peers_lock = peers_inner.write().await;
                    if peers_lock.contains_key(&remote_id) {
                        debug!("[P2P] Already bonded to peer {}", remote_id);
                        return;
                    }
                    if peers_lock.len() >= MAX_PEERS {
                        info!("[P2P] Max peers reached ({}), dropping bonded peer {}", MAX_PEERS, remote_id);
                        return;
                    }
                    
                    info!("[P2P] Saving peer record: ID={}, Addr={}", remote_id, addr);
                    peers_lock.insert(remote_id.clone(), PeerInfo { 
                        addr, 
                        rpc_url: rpc_url.clone() 
                    });
                    info!("[P2P] Active peer pool size: {}", peers_lock.len());
                }
                Err(e) => error!("[P2P] Bonding failed with {}: {}", addr, e),
            }
        });
    }

    pub async fn broadcast_gossip(&self, data: Vec<u8>) {
        let peers = self.peers.read().await;
        let mut target_urls = Vec::new();
        for (id, info) in peers.iter() {
            target_urls.push((id.clone(), info.rpc_url.clone()));
        }
        drop(peers);

        for (id, url) in target_urls {
            let data_clone = data.clone();
            tokio::spawn(async move {
                let client = RpcClient::new(url);
                if let Err(e) = client.gossip("gossip".to_string(), data_clone).await {
                    error!("[P2P] Failed to send gossip to {}: {}", id, e);
                }
            });
        }
    }

    pub async fn send_peer_list(&self, _peer_id: &str) {
        // In RPC model, peers poll for lists, or we could push them.
        // For now, we rely on discover_peers polling.
    }
}
