pub mod discovery;
pub mod protocol;
pub mod service;
pub mod sync;

use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use crate::storage::{InMemoryStorage, Transaction, Block};
use crate::network::protocol::{BlockHeaders, BlockBodies};

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub discv5_addr: std::net::SocketAddr,
    pub p2p_addr: std::net::SocketAddr,
    pub ext_ip: Option<std::net::IpAddr>,
    pub bootnodes: Vec<String>,
    pub max_peers: usize,
}

pub enum NetworkMessage {
    GetPeerCount(oneshot::Sender<usize>),
    GetPeers(oneshot::Sender<Vec<PeerInfo>>),
    AddPeer(String, oneshot::Sender<Result<(), String>>),
    GetNodeInfo(oneshot::Sender<NodeInfo>),
    BroadcastBlock(Block),
    RequestHeaders {
        session_id: tentacle::SessionId,
        request: crate::network::protocol::GetBlockHeaders,
    },
    RequestBodies {
        session_id: tentacle::SessionId,
        request: crate::network::protocol::GetBlockBodies,
    },
    SyncHeaders(tentacle::SessionId, BlockHeaders),
    SyncBodies(tentacle::SessionId, BlockBodies),
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: String,
    pub addr: String,
    pub enr: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub enr: String,
    pub node_id: String,
    pub listen_addresses: Vec<String>,
}

pub struct NetworkHandle {
    pub tx_broadcast: mpsc::Sender<Transaction>,
    pub network_send: mpsc::Sender<NetworkMessage>,
}

pub async fn start_network(
    config: NetworkConfig,
    storage: Arc<Mutex<InMemoryStorage>>,
) -> Result<NetworkHandle, Box<dyn std::error::Error>> {
    let (tx_broadcast, rx_broadcast) = mpsc::channel(100);
    let (network_send, network_recv) = mpsc::channel(100);
    
    let (sync_service, sync_send) = sync::SyncService::new(storage.clone(), network_send.clone());
    
    println!("[network::mod] Starting network with config: {:?}", config.discv5_addr);
    let service = match service::NetworkService::new(config, storage, rx_broadcast, network_recv, sync_send).await {
        Ok(s) => {
            println!("[network::mod] NetworkService instance created successfully.");
            s
        }
        Err(e) => {
            println!("[network::mod] Error creating NetworkService: {:?}", e);
            return Err(e);
        }
    };

    tokio::spawn(async move {
        println!("[network::mod] Spawning network service loop...");
        if let Err(e) = service.run().await {
            println!("[network::mod] Network service error during run: {:?}", e);
        }
        println!("[network::mod] Network service loop terminated.");
    });

    tokio::spawn(async move {
        sync_service.run().await;
    });

    Ok(NetworkHandle { tx_broadcast, network_send })
}
