pub mod discovery;
pub mod protocol;
pub mod service;

use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use crate::storage::{InMemoryStorage, Transaction};

#[derive(Debug)]
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
    
    println!("[network::mod] Starting network with config: {:?}", config.discv5_addr);
    let service = match service::NetworkService::new(config, storage, rx_broadcast).await {
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

    Ok(NetworkHandle { tx_broadcast })
}
