pub mod discovery;
pub mod protocol;
pub mod service;

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::storage::InMemoryStorage;

pub struct NetworkConfig {
    pub discv5_addr: std::net::SocketAddr,
    pub p2p_addr: std::net::SocketAddr,
    pub bootnodes: Vec<String>,
}

pub async fn start_network(
    config: NetworkConfig,
    storage: Arc<Mutex<InMemoryStorage>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let service = service::NetworkService::new(config, storage).await?;
    tokio::spawn(async move {
        if let Err(e) = service.run().await {
            tracing::error!("Network service error: {:?}", e);
        }
    });
    Ok(())
}
