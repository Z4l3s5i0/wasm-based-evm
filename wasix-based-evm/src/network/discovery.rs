use discv5::{Discv5, ConfigBuilder, Enr, enr::CombinedKey, ListenConfig};
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;

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
        let mut enr_builder = Enr::builder();
        match listen_addr.ip() {
            IpAddr::V4(ip) => {
                enr_builder.ip4(ip);
                enr_builder.udp4(listen_addr.port());
            }
            IpAddr::V6(ip) => {
                enr_builder.ip6(ip);
                enr_builder.udp6(listen_addr.port());
            }
        }
        println!("[DiscoveryService] Building local ENR...");
        let enr = enr_builder.build(&enr_key)?;
        println!("[DiscoveryService] Local ENR: {}", enr.to_base64());
        println!("[DiscoveryService] Node ID: {}", enr.node_id());

        // Configure discv5
        println!("[DiscoveryService] Configuring discv5...");
        let listen_config = match listen_addr.ip() {
            IpAddr::V4(ip) => {
                ListenConfig::default().with_ipv4(ip, listen_addr.port())
            }
            IpAddr::V6(ip) => {
                ListenConfig::default().with_ipv6(ip, listen_addr.port())
            }
        };
        
        let config = ConfigBuilder::new(listen_config).build();
        println!("[DiscoveryService] Creating Discv5 instance...");
        let discv5 = Discv5::new(enr, enr_key, config)?;
        println!("[DiscoveryService] Discv5 instance created.");

        // Add bootnodes
        for bootnode in bootnodes {
            match Enr::from_str(&bootnode) {
                Ok(enr) => {
                    if let Err(e) = discv5.add_enr(enr) {
                        println!("Failed to add bootnode ENR: {:?}", e);
                    }
                }
                Err(e) => println!("Invalid bootnode ENR: {:?}, error: {:?}", bootnode, e),
            }
        }

        Ok(Self { discv5 })
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[DiscoveryService] Starting discv5...");
        println!("[DiscoveryService] Local ENR for start: {}", self.discv5.local_enr().to_base64());
        match self.discv5.start().await {
            Ok(_) => {
                println!("[DiscoveryService] Discv5 started successfully.");
                Ok(())
            }
            Err(e) => {
                println!("[DiscoveryService] Failed to start discv5: {:?}", e);
                Err(format!("{:?}", e).into())
            }
        }
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
