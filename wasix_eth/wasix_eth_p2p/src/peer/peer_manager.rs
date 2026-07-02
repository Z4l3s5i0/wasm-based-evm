use std::net::SocketAddr;
use std::sync::Arc;
use crate::peer::peer_registry::PeerRegistry;
use crate::peer::dialer::PeerDialer;
use crate::peer::discovery_manager::DiscoveryManager;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_utils::identity::Identity;
use wasix_eth_types::ChainConfig;
use alloy_primitives::B256;
use tokio::sync::mpsc;
use crate::discovery::v4_service::DiscoveryV4Service;

pub struct PeerManager {
    pub registry: Arc<PeerRegistry>,
    pub dialer: Arc<PeerDialer>,
    pub discovery_manager: Arc<DiscoveryManager>,
}

impl PeerManager {
    pub fn new(
        read_provider: DatabaseReadProvider,
        write_provider: DatabaseWriteProvider,
        local_identity: Identity,
        discovery_port: u16,
        p2p_port: u16,
        ext_ip: Option<std::net::IpAddr>,
        bootnodes: Vec<String>,
        network_id: u64,
        genesis_hash: B256,
        chain_config: ChainConfig,
    ) -> Self {
        let registry = Arc::new(PeerRegistry::new(
            read_provider.clone(),
            write_provider.clone(),
            local_identity.clone(),
            discovery_port,
            p2p_port,
            ext_ip,
            bootnodes.clone(),
            network_id,
            genesis_hash,
            chain_config,
        ));

        let dialer = Arc::new(PeerDialer::new(
            registry.clone(),
            write_provider,
        ));

        let discovery_manager = Arc::new(DiscoveryManager::new(
            registry.clone(),
            dialer.clone(),
            bootnodes,
        ));

        Self {
            registry,
            dialer,
            discovery_manager,
        }
    }

    pub async fn start(&self, discovery_service: Option<Arc<DiscoveryV4Service>>) {
        if let Some(service) = discovery_service {
            self.registry.set_discovery_service_v4(service.clone()).await;
            self.discovery_manager.start(Some(service)).await;
        } else {
            self.discovery_manager.start(None).await;
        }

        // Start Health Check Loop
        let registry = self.registry.clone();
        tokio::spawn(async move {
            loop {
                registry.cleanup_stale_peers().await;
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            }
        });
    }

    pub async fn set_gossip_tx(&self, tx: mpsc::Sender<wasix_eth_types::p2p::GossipMessage>) {
        self.registry.set_gossip_tx(tx).await;
    }

    pub fn dial_peer(&self, addr: SocketAddr) {
        self.dialer.dial_peer(addr);
    }

    pub fn dial_enode(&self, enode_str: &str) {
        self.dialer.dial_enode(enode_str);
    }
}
