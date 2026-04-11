use discv5::{Discv5, ConfigBuilder, Event, ListenConfig};
use discv5::enr::{CombinedKey, Enr as RawEnr, NodeId};
use std::net::SocketAddr;
use std::sync::Arc;
use crate::{info, error, debug};
use anyhow::Result;

pub struct DiscoveryService {
    discv5: Arc<Discv5>,
}

impl DiscoveryService {
    pub async fn new(
        identity_key: CombinedKey,
        local_enr: RawEnr<CombinedKey>,
        listen_port: u16,
        bootnodes: Vec<String>,
    ) -> Result<Self> {
        let listen_addr = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), listen_port);
        let listen_config = ListenConfig::from(listen_addr);
        
        let config = ConfigBuilder::new(listen_config)
            .build();

        let mut discv5 = Discv5::new(local_enr, identity_key, config)
            .map_err(|e| anyhow::anyhow!("Failed to initialize Discv5: {}", e))?;

        for enr_str in bootnodes {
            match enr_str.parse::<RawEnr<CombinedKey>>() {
                Ok(enr) => {
                    if let Err(e) = discv5.add_enr(enr) {
                        error!("[Discovery] Failed to add bootnode ENR: {}", e);
                    }
                }
                Err(e) => {
                    error!("[Discovery] Failed to parse bootnode ENR '{}': {}", enr_str, e);
                }
            }
        }

        // Start the service before wrapping it in Arc
        discv5.start().await
            .map_err(|e| anyhow::anyhow!("Failed to start Discv5: {}", e))?;
        
        Ok(Self { discv5: Arc::new(discv5) })
    }

    pub async fn start(&self) -> Result<()> {
        info!("[Discovery] Starting background tasks...");
        
        let mut event_stream = self.discv5.event_stream().await
            .map_err(|e| anyhow::anyhow!("Failed to get Discv5 event stream: {}", e))?;

        // Active discovery loop
        let discv5_clone = Arc::clone(&self.discv5);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                debug!("[Discovery] Triggering random DHT query...");
                let target_node = NodeId::random();
                let found_nodes = discv5_clone.find_node(target_node).await;
                match found_nodes {
                    Ok(nodes) => {
                        if !nodes.is_empty() {
                            info!("[Discovery] DHT query found {} nodes", nodes.len());
                        }
                    }
                    Err(e) => debug!("[Discovery] DHT query failed: {}", e),
                }
            }
        });

        // Event handler loop
        tokio::spawn(async move {
            while let Some(event) = event_stream.recv().await {
                match event {
                    Event::Discovered(enr) => {
                        info!("[Discovery] Peer discovered: {}", enr);
                    }
                    Event::NodeInserted { node_id, replaced } => {
                        debug!("[Discovery] Node inserted: {} (replaced: {:?})", node_id, replaced);
                    }
                    Event::SocketUpdated(addr) => {
                        info!("[Discovery] External socket updated: {}", addr);
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    pub fn local_enr(&self) -> RawEnr<CombinedKey> {
        self.discv5.local_enr()
    }
}
