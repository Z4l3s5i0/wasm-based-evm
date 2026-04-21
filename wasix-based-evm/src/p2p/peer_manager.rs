use crate::p2p::{DiscoveryApiServer, P2pApiServer, HelloResponse, PeerInfoRlp, DiscoveryApiClient, P2pApiClient};
use crate::p2p::rpc_client::RpcClient;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use alloy_primitives::B256;
use tokio::sync::RwLock;
use jsonrpsee::server::ServerBuilder;
use jsonrpsee::core::RpcResult;
use crate::{info, error, debug};
use tokio::sync::mpsc;

use crate::storage::storage::InMemoryStorage;
use alloy_rlp::Encodable;
use crate::identity::identity::Identity;

const MAX_PEERS: usize = 50;

#[derive(Clone)]
pub struct PeerInfo {
    pub discovery_addr: SocketAddr,
    pub p2p_addr: SocketAddr,
    pub discovery_url: String,
    pub p2p_url: String,
}

#[derive(Clone)]
pub struct PeerManager {
    local_identity: Identity,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    gossip_tx: mpsc::Sender<Vec<u8>>,
    storage: Arc<RwLock<InMemoryStorage>>,
    pub discovery_port: u16,
    pub p2p_port: u16,
    pub ext_ip: Option<std::net::IpAddr>,
    pub bootnodes: Vec<String>,
}

#[async_trait::async_trait]
impl DiscoveryApiServer for PeerManager {
    async fn hello(&self, peer_id: String, discovery_addr: String, p2p_addr: String) -> RpcResult<HelloResponse> {
        debug!("[P2P] Received hello from {} (discovery_addr: {}, p2p_addr: {})", peer_id, discovery_addr, p2p_addr);
        
        let peers_lock = self.peers.read().await;
        let is_already_bonded = peers_lock.contains_key(&peer_id);
        drop(peers_lock);

        if !is_already_bonded {
            if let Ok(addr) = discovery_addr.parse::<SocketAddr>() {
                let pm = self.clone();
                tokio::spawn(async move {
                    Arc::new(pm).dial_peer(addr);
                });
            }
        } else {
            debug!("[P2P] Already bonded to peer {}, skipping reciprocal dial", peer_id);
        }
        
        Ok(HelloResponse {
            peer_id: self.local_identity.peer_id(),
            discovery_addr: format!("{}:{}", self.ext_ip.unwrap_or_else(|| "127.0.0.1".parse().unwrap()), self.discovery_port),
            p2p_addr: format!("{}:{}", self.ext_ip.unwrap_or_else(|| "127.0.0.1".parse().unwrap()), self.p2p_port),
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
                discovery_addr: peer_info.discovery_addr.to_string(),
                p2p_addr: peer_info.p2p_addr.to_string(),
            });
            if list.len() >= 16 {
                break;
            }
        }
        Ok(list)
    }
}

#[async_trait::async_trait]
impl P2pApiServer for PeerManager {
    async fn get_block_by_number(&self, number: u64) -> RpcResult<Option<Vec<u8>>> {
        let storage = self.storage.read().await;
        if let Some(block) = storage.get_block_by_number(number) {
            let mut out = Vec::new();
            block.encode(&mut out);
            Ok(Some(out))
        } else {
            Ok(None)
        }
    }

    async fn get_block_hash(&self, number: u64) -> RpcResult<Option<B256>> {
        let storage = self.storage.read().await;
        Ok(storage.get_block_hash(number))
    }

    async fn get_block_by_hash(&self, hash: B256) -> RpcResult<Option<Vec<u8>>> {
        let storage = self.storage.read().await;
        match storage.get_block_by_hash(hash) {
            Some(block) => {
                let mut buf = Vec::new();
                alloy_rlp::Encodable::encode(&block, &mut buf);
                Ok(Some(buf))
            }
            None => Ok(None),
        }
    }

    async fn gossip(&self, _topic: String, data: Vec<u8>) -> RpcResult<()> {
        debug!("[P2P] Received gossip message ({} bytes)", data.len());
        let _ = self.gossip_tx.send(data).await;
        Ok(())
    }

    async fn block_number(&self) -> RpcResult<String> {
        let storage = self.storage.read().await;
        let num = storage.get_latest_block_number();
        Ok(format!("0x{:x}", num))
    }
}

impl PeerManager {
    pub fn new(
        identity: Identity,
        storage: Arc<RwLock<InMemoryStorage>>,
        discovery_port: u16,
        p2p_port: u16,
        ext_ip: Option<std::net::IpAddr>,
        bootnodes: Vec<String>,
    ) -> Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let (gossip_tx, gossip_rx) = mpsc::channel(100);
        Ok((Self {
            local_identity: identity,
            peers: Arc::new(RwLock::new(HashMap::new())),
            gossip_tx,
            storage,
            discovery_port,
            p2p_port,
            ext_ip,
            bootnodes,
        }, gossip_rx))
    }

    pub async fn get_active_peers(&self) -> HashMap<String, PeerInfo> {
        self.peers.read().await.clone()
    }

    pub async fn start(
        &self,
        discovery_port: u16,
        bootnodes: Vec<String>,
    ) -> Result<()> {
        // Start Discovery Server
        let bind_ip = self.ext_ip.unwrap_or_else(|| "127.0.0.1".parse().unwrap());
        let discovery_addr = SocketAddr::new(bind_ip, discovery_port);
        let discovery_server = ServerBuilder::default().build(discovery_addr).await?;
        let discovery_handle = discovery_server.start(DiscoveryApiServer::into_rpc(self.clone()));
        info!("[P2P] Discovery RPC Server started on {}", discovery_addr);
        tokio::spawn(discovery_handle.stopped());

        // Start P2P Server
        let p2p_addr = SocketAddr::new(bind_ip, self.p2p_port);
        let p2p_server = ServerBuilder::default().build(p2p_addr).await?;
        let p2p_handle = p2p_server.start(P2pApiServer::into_rpc(self.clone()));
        info!("[P2P] P2P RPC Server started on {}", p2p_addr);
        tokio::spawn(p2p_handle.stopped());

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
            .map(|(id, info)| (id.clone(), info.discovery_url.clone()))
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
                        if let Ok(addr) = p.discovery_addr.parse::<SocketAddr>() {
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
            .map(|(id, info)| (id.clone(), info.discovery_url.clone()))
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
                        debug!("[P2P] Peer {} returned unexpected response: {}", id, res);
                        to_remove.push(id);
                    }
                }
                Err(e) => {
                    debug!("[P2P] Peer {} is unreachable: {}. Removing from active pool.", id, e);
                    to_remove.push(id);
                }
            }
        }

        if !to_remove.is_empty() {
            let mut peers_lock = self.peers.write().await;
            for id in to_remove {
                peers_lock.remove(&id);
                debug!("[P2P] Successfully removed stale peer {}", id);
            }
            debug!("[P2P] Current active peer count: {}", peers_lock.len());
        } else {
            debug!("[P2P] All peers are healthy.");
        }
    }

    pub fn dial_peer(self: Arc<Self>, addr: SocketAddr) {
        let local_id = self.local_identity.peer_id();
        let peers_inner = self.peers.clone();
        let discovery_port = self.discovery_port;
        let p2p_port = self.p2p_port;
        let ext_ip = self.ext_ip;
        
        tokio::spawn(async move {
            // Pre-check: Is this address already in our peer pool?
            let peers_lock = peers_inner.read().await;
            for (id, info) in peers_lock.iter() {
                if info.discovery_addr == addr {
                    debug!("[P2P] Already bonded to peer at {} (ID: {}), skipping dial", addr, id);
                    return;
                }
            }
            drop(peers_lock);

            debug!("[P2P] Attempting to bond with peer at {}", addr);
            let discovery_url = format!("http://{}", addr);
            
            // Determine our own public address to share
            let my_public_ip = ext_ip.unwrap_or_else(|| "127.0.0.1".parse().unwrap());
            let my_discovery_addr = format!("{}:{}", my_public_ip, discovery_port);
            let my_p2p_addr = format!("{}:{}", my_public_ip, p2p_port);

            let client = RpcClient::new(discovery_url.clone());
            match client.hello(local_id, my_discovery_addr, my_p2p_addr).await {
                Ok(resp) => {
                    let remote_id = resp.peer_id;
                    debug!("[P2P] Bonding successful with {} (PeerId: {})", addr, remote_id);
                    
                    let mut peers_lock = peers_inner.write().await;
                    if peers_lock.contains_key(&remote_id) {
                        debug!("[P2P] Already bonded to peer {}", remote_id);
                        return;
                    }
                    if peers_lock.len() >= MAX_PEERS {
                        debug!("[P2P] Max peers reached ({}), dropping bonded peer {}", MAX_PEERS, remote_id);
                        return;
                    }
                    
                    let discovery_addr = addr;
                    let p2p_addr: SocketAddr = resp.p2p_addr.parse().unwrap_or_else(|_| {
                         let mut a = addr;
                         a.set_port(9002); // fallback
                         a
                    });
                    let p2p_url = format!("http://{}", p2p_addr);

                    debug!("[P2P] Saving peer record: ID={}, DiscoveryAddr={}, P2pAddr={}", remote_id, discovery_addr, p2p_addr);
                    peers_lock.insert(remote_id.clone(), PeerInfo { 
                        discovery_addr,
                        p2p_addr,
                        discovery_url: discovery_url.clone(),
                        p2p_url,
                    });
                    debug!("[P2P] Active peer pool size: {}", peers_lock.len());
                }
                Err(e) => error!("[P2P] Bonding failed with {}: {}", addr, e),
            }
        });
    }

    pub async fn broadcast_gossip(&self, data: Vec<u8>) {
        let peers = self.peers.read().await;
        let mut target_urls = Vec::new();
        for (id, info) in peers.iter() {
            target_urls.push((id.clone(), info.p2p_url.clone()));
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

}
