use std::sync::Arc;
use std::net::SocketAddr;
use tokio::time::{Duration, sleep};
use crate::peer::peer_registry::PeerRegistry;
use crate::peer::dialer::PeerDialer;
use crate::discovery::v4_service::DiscoveryV4Service;

pub struct DiscoveryManager {
    registry: Arc<PeerRegistry>,
    dialer: Arc<PeerDialer>,
    bootnodes: Vec<String>,
}

impl DiscoveryManager {
    pub fn new(
        registry: Arc<PeerRegistry>,
        dialer: Arc<PeerDialer>,
        bootnodes: Vec<String>,
    ) -> Self {
        Self {
            registry,
            dialer,
            bootnodes,
        }
    }

    pub async fn start(&self, discovery_service: Option<Arc<DiscoveryV4Service>>) {
        if let Some(service) = discovery_service {
            service.start().await;
        }

        // Initial bootstrap dials
        for bootnode in &self.bootnodes {
            self.dialer.dial_enode(bootnode);
        }

        // Start Peer Connector loop
        let registry = self.registry.clone();
        let dialer = self.dialer.clone();
        tokio::spawn(async move {
            loop {
                // Periodically check for new peers found via discovery
                if let Some(service) = registry.get_discovery_service_v4().await {
                    let discovered = service.get_all_nodes().await;
                    let local_peer_id = registry.local_peer_id();
                    for node in discovered {
                        if format!("{:?}", node.id) == local_peer_id {
                            continue;
                        }
                        let addr = SocketAddr::new(node.endpoint.ip, node.endpoint.tcp_port);
                        dialer.dial_peer(addr);
                    }
                }

                sleep(Duration::from_secs(30)).await;
            }
        });
    }
}
