use crate::{info, error, debug};
use crate::p2p::connection::{Connection, Message, HelloMessage, PeerInfoRlp};
use crate::p2p::identity::Identity;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

const MAX_PEERS: usize = 50;

pub struct PeerInfo {
    pub tx: mpsc::Sender<Message>,
    pub addr: Option<SocketAddr>,
}

#[derive(Clone)]
pub struct PeerManager {
    local_identity: Identity,
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
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
        let listen_port = self.port;
        let pm_inner_loop = self.clone();

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
                        let pm_conn = pm_inner_loop.clone();
                        
                        tokio::spawn(async move {
                            match Connection::new_server(stream, local_id, Some(listen_port), rx).await {
                                Ok((conn, remote_id, remote_listen_port)) => {
                                    info!("[P2P] Handshake successful (No TLS) with {} at {}", remote_id, addr);
                                    
                                    let mut remote_addr = None;
                                    if let Some(port) = remote_listen_port {
                                        let mut sa = addr;
                                        sa.set_port(port);
                                        remote_addr = Some(sa);
                                    }

                                    // Register peer
                                    {
                                        let mut peers_lock = peers_inner.write().await;
                                        if peers_lock.len() >= MAX_PEERS {
                                            info!("[P2P] Max peers reached, dropping {}", remote_id);
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

                                    if let Err(e) = conn.process(pm_conn).await {
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
