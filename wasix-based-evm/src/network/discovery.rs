use std::sync::Arc;
use tokio::sync::Mutex;
use discv5::{Discv5, ConfigBuilder, Enr, enr::CombinedKey, ListenConfig, enr::NodeId};
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;
use crate::{info, debug};

pub struct DiscoveryService {
    discv5: Arc<Mutex<Discv5>>,
    bootnodes: Vec<String>,
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
        let mut public_ip = ext_ip.unwrap_or(listen_addr.ip());
        
        // If the public IP is unspecified (0.0.0.0 or ::), fallback to 127.0.0.1
        // to ensure it's at least reachable locally.
        if public_ip.is_unspecified() {
            public_ip = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
        }
        
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
        //3
        debug!("[DiscoveryService] Building local ENR...");
        let enr = enr_builder.build(&enr_key)?;
        info!("[DiscoveryService] Local ENR: {}", enr.to_base64());
        info!("[DiscoveryService] Node ID: {}", enr.node_id());

        // Configure discv5
        debug!("[DiscoveryService] Configuring discv5...");
        let listen_config = match listen_addr.ip() {
            IpAddr::V4(_ip) => {
                // For small local networks/tests, bind explicitly to localhost to avoid firewall issues
                ListenConfig::default().with_ipv4(std::net::Ipv4Addr::LOCALHOST, listen_addr.port())
            }
            IpAddr::V6(_ip) => {
                ListenConfig::default().with_ipv6(std::net::Ipv6Addr::LOCALHOST, listen_addr.port())
            }
        };
        
        let config = ConfigBuilder::new(listen_config)
            .request_timeout(std::time::Duration::from_secs(10))
            .query_peer_timeout(std::time::Duration::from_secs(5))
            .query_timeout(std::time::Duration::from_secs(30))
            .request_retries(3)
            .query_parallelism(5)
            .build();
        debug!("[DiscoveryService] Creating Discv5 instance...");
        let discv5_raw = Discv5::new(enr, enr_key, config)?;
        debug!("[DiscoveryService] Discv5 instance created.");

        // Add bootnodes
        for bootnode in &bootnodes {
            match Enr::from_str(bootnode) {
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
        
        Ok(Self { discv5, bootnodes })
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
        debug!("[DiscoveryService] Triggering peer discovery...");
        
        let discv5_clone = self.discv5_clone();
        let bootnodes = self.bootnodes.clone();
        tokio::spawn(async move {
            let (nodes, local_id) = {
                let discv5 = discv5_clone.lock().await;
                (discv5.table_entries_enr(), discv5.local_enr().node_id())
            };
            
            // Periodically refresh the local ENR to keep its sequence number fresh
            // or if we ever want to update its contents. Discv5 will handle the seq update.
            {
                let discv5 = discv5_clone.lock().await;
                // Re-inserting an existing field (like "udp") will increment the ENR sequence number.
                let current_udp = discv5.local_enr().udp4();
                if let Some(udp_port) = current_udp {
                    if let Err(e) = discv5.enr_insert("udp", &udp_port) {
                        debug!("[DiscoveryService] Failed to update local ENR sequence via enr_insert: {:?}", e);
                    }
                }
            }
            
            // If table is empty, try to re-add bootnodes
            if nodes.is_empty() {
                debug!("[DiscoveryService] Routing table is empty, re-adding bootnodes...");
                let discv5 = discv5_clone.lock().await;
                for bootnode in &bootnodes {
                    if let Ok(enr) = Enr::from_str(bootnode) {
                        let _ = discv5.add_enr(enr);
                    }
                }
            }

            debug!("[DiscoveryService] Pinging {} known nodes...", nodes.len());
            for enr in nodes {
                let discv5 = discv5_clone.lock().await;
                let _ = discv5.send_ping(enr).await;
            }

            // Query for our own ID to refresh our bucket
            debug!("[DiscoveryService] Refreshing local bucket...");
            let find_self_future = {
                let discv5 = discv5_clone.lock().await;
                discv5.find_node(local_id)
            };
            let _ = find_self_future.await;

            // Also query for bootnode IDs if available to help small network convergence
            let bootnode_ids: Vec<NodeId> = {
                let mut ids = Vec::new();
                for bn in &bootnodes {
                    if let Ok(enr) = Enr::from_str(bn) {
                        ids.push(enr.node_id());
                    }
                }
                ids
            };
            
            for bn_id in bootnode_ids {
                debug!("[DiscoveryService] Querying for bootnode {}...", bn_id);
                let find_bn_future = {
                    let discv5 = discv5_clone.lock().await;
                    discv5.find_node(bn_id)
                };
                let _ = find_bn_future.await;
            }

            // Also query for a random target
            let target_node = NodeId::random();
            let find_node_future = {
                let discv5 = discv5_clone.lock().await;
                discv5.find_node(target_node)
            };

            match find_node_future.await {
                Ok(nodes) => {
                    debug!("[DiscoveryService] find_node query finished. Found {} nodes.", nodes.len());
                }
                Err(e) => {
                    debug!("[DiscoveryService] find_node query failed: {:?}", e);
                }
            }
        });
    }
}
