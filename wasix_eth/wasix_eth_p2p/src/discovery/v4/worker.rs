use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use wasix_eth_utils::info;
use alloy_primitives::B512;
use crate::discovery::v4_service::DiscoveryV4Service;

pub struct DiscoveryWorker {
    service: Arc<DiscoveryV4Service>,
}

impl DiscoveryWorker {
    pub fn new(service: Arc<DiscoveryV4Service>) -> Self {
        Self { service }
    }

    pub async fn start(self: Arc<Self>) {
        let worker = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                worker.re_lookup().await;
            }
        });
    }

    async fn re_lookup(&self) {
        let nodes = {
            let rt = self.service.routing_table.lock().await;
            rt.get_all_nodes()
        };

        let now = std::time::Instant::now();
        let timeout = Duration::from_secs(3600);

        for node in nodes {
            let addr = SocketAddr::new(node.endpoint.ip, node.endpoint.udp_port);
            
            if now.duration_since(node.last_seen) > timeout {
                info!("[DiscoveryV4] Removing stale node {} (ID: {})", addr, node.id);
                self.service.routing_table.lock().await.remove_node(node.id);
                continue;
            }

            let _ = self.service.ping_node(addr).await;
            
            // Also FindNode to discover more
            let uncompressed = self.service.local_identity.keypair.verifying_key().to_encoded_point(false);
            let mut id_bytes = [0u8; 64];
            id_bytes.copy_from_slice(&uncompressed.as_bytes()[1..]);
            let target = B512::from(id_bytes); // Lookup self to fill buckets near us

            let _ = self.service.send_find_node(addr, target).await;

            // Yield between nodes to avoid packet bursts
            tokio::task::yield_now().await;
        }
    }
}
