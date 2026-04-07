pub mod discovery;
pub mod protocol;
pub mod service;

use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use crate::storage::{InMemoryStorage, Transaction};

pub struct NetworkConfig {
    pub discv5_addr: std::net::SocketAddr,
    pub p2p_addr: std::net::SocketAddr,
    pub bootnodes: Vec<String>,
}

pub struct NetworkHandle {
    pub tx_broadcast: mpsc::Sender<Transaction>,
}

pub async fn start_network(
    config: NetworkConfig,
    storage: Arc<Mutex<InMemoryStorage>>,
) -> Result<NetworkHandle, Box<dyn std::error::Error>> {
    let (tx_broadcast, rx_broadcast) = mpsc::channel(100);
    
    let service = service::NetworkService::new(config, storage, rx_broadcast).await?;
    tokio::spawn(async move {
        if let Err(e) = service.run().await {
            println!("Network service error: {:?}", e);
        }
    });

    Ok(NetworkHandle { tx_broadcast })
}
