use std::sync::Arc;
use tokio::sync::Mutex;
use discv5::{Discv5, ConfigBuilder, Enr, enr::CombinedKey, ListenConfig};
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;

pub struct DiscoveryService {
    discv5: Arc<Mutex<Discv5>>,
}

impl DiscoveryService {
    pub fn new(
        listen_addr: SocketAddr,
        p2p_port: u16,
        ext_ip: Option<IpAddr>,
        bootnodes: Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate a random key for the local ENR
        let enr_key = CombinedKey::generate_secp256k1();
        
        // Build the local ENR
        let mut enr_builder = Enr::builder();
        
        // Use external IP if provided, otherwise fallback to listen IP
        let public_ip = ext_ip.unwrap_or(listen_addr.ip());
        
        match public_ip {
            IpAddr::V4(ip) => {
                enr_builder.ip4(ip);
                enr_builder.udp4(listen_addr.port());
                enr_builder.tcp4(p2p_port);
            }
            IpAddr::V6(ip) => {
                enr_builder.ip6(ip);
                enr_builder.udp6(listen_addr.port());
                enr_builder.tcp6(p2p_port);
            }
        }
        debug!("[DiscoveryService] Building local ENR...");
        let enr = enr_builder.build(&enr_key)?;
        info!("[DiscoveryService] Local ENR: {}", enr.to_base64());
        info!("[DiscoveryService] Node ID: {}", enr.node_id());

        // Configure discv5
        debug!("[DiscoveryService] Configuring discv5...");
        let listen_config = match listen_addr.ip() {
            IpAddr::V4(ip) => {
                ListenConfig::default().with_ipv4(ip, listen_addr.port())
            }
            IpAddr::V6(ip) => {
                ListenConfig::default().with_ipv6(ip, listen_addr.port())
            }
        };
        
        let config = ConfigBuilder::new(listen_config).build();
        debug!("[DiscoveryService] Creating Discv5 instance...");
        let discv5_raw = Discv5::new(enr, enr_key, config)?;
        debug!("[DiscoveryService] Discv5 instance created.");

        // Add bootnodes
        for bootnode in bootnodes {
            match Enr::from_str(&bootnode) {
                Ok(enr) => {
                    if let Err(e) = discv5_raw.add_enr(enr.clone()) {
                        info!("[DiscoveryService] Failed to add bootnode ENR: {:?}", e);
                    }
                    debug!("[DiscoveryService] Successfully added bootnode ENR: {:?}", enr);
                }
                Err(e) => info!("[DiscoveryService] Invalid bootnode ENR: {:?}, error: {:?}", bootnode, e),
            }
        }

        let discv5 = Arc::new(Mutex::new(discv5_raw));
        
        Ok(Self { discv5 })
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        debug!("[DiscoveryService] Starting discv5...");
        let local_enr = {
            let discv5 = self.discv5.lock().await;
            discv5.local_enr().to_base64()
        };
        debug!("[DiscoveryService] Local ENR for start: {}", local_enr);
        let mut discv5 = self.discv5.lock().await;
        match discv5.start().await {
            Ok(_) => {
                info!("[DiscoveryService] Discv5 started successfully.");
                Ok(())
            }
            Err(e) => {
                info!("[DiscoveryService] Failed to start discv5: {:?}", e);
                Err(format!("{:?}", e).into())
            }
        }
    }

    pub async fn discv5_local_enr(&self) -> Enr {
        let discv5 = self.discv5.lock().await;
        discv5.local_enr().clone()
    }
    
    pub fn discv5_clone(&self) -> Arc<Mutex<Discv5>> {
        Arc::clone(&self.discv5)
    }

    pub async fn bootnodes(&self) -> Vec<Enr> {
        let discv5 = self.discv5.lock().await;
        discv5.table_entries_enr()
    }

    pub async fn add_enr(&self, enr: Enr) -> Result<(), String> {
        let discv5 = self.discv5.lock().await;
        discv5.add_enr(enr).map_err(|e| format!("{:?}", e))
    }

    pub async fn event_stream(&self) -> Result<tokio::sync::mpsc::Receiver<discv5::Event>, String> {
        let discv5 = self.discv5.lock().await;
        discv5.event_stream().await.map_err(|e| format!("{:?}", e))
    }

    pub async fn find_peers(&self) {
        // Simple periodic peer discovery could be implemented here
    }
}
