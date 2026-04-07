use discv5::{Discv5, ConfigBuilder, Enr, enr::CombinedKey, ListenConfig};
use std::net::SocketAddr;
use std::str::FromStr;
use tracing::{info, warn};

pub struct DiscoveryService {
    discv5: Discv5,
}

impl DiscoveryService {
    pub fn new(
        listen_addr: SocketAddr,
        bootnodes: Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate a random key for the local ENR
        let enr_key = CombinedKey::generate_secp256k1();
        
        // Build the local ENR
        let enr = Enr::builder()
            .ip(listen_addr.ip())
            .udp4(listen_addr.port())
            .build(&enr_key)?;

        info!("Local ENR: {}", enr.to_base64());
        info!("Node ID: {}", enr.node_id());

        // Configure discv5
        let listen_config = ListenConfig::from(listen_addr);
        let config = ConfigBuilder::new(listen_config).build();
        let discv5 = Discv5::new(enr, enr_key, config)?;

        // Add bootnodes
        for bootnode in bootnodes {
            match Enr::from_str(&bootnode) {
                Ok(enr) => {
                    if let Err(e) = discv5.add_enr(enr) {
                        warn!("Failed to add bootnode ENR: {:?}", e);
                    }
                }
                Err(e) => warn!("Invalid bootnode ENR: {:?}, error: {:?}", bootnode, e),
            }
        }

        Ok(Self { discv5 })
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.discv5.start().await.map_err(|e| format!("{:?}", e).into())
    }

    pub fn discv5(&self) -> &Discv5 {
        &self.discv5
    }

    pub async fn find_peers(&self) {
        // Simple periodic peer discovery could be implemented here
        // For now, we rely on the discv5 internal routing table and bootstrapping
        // bootstrap() was removed or renamed in recent versions, use query_nodes or similar if needed
        // but discv5 usually starts finding peers automatically if bootnodes are added.
    }
}
