use wasix_eth_core::gossip::gossip_bridge::GossipBridge;
use wasix_eth_core::gossip::GossipService;
use wasix_eth_core::sync::registry::SyncRegistry;
use wasix_eth_core::{Engine, ChainManager};
use wasix_eth_p2p::{SyncService, PeerManager};
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_types::sync::SyncProvider;
use wasix_eth_core::sync::sync_orchestrator::SyncOrchestrator;
use std::sync::Arc;
use tokio::task::JoinHandle;
use wasix_eth_core::sync::controller::SyncController;
use wasix_eth_core::sync::processor::BlockProcessor;

pub struct SyncPayload {
    pub sync_controller: Arc<SyncController>,
    pub sync_orchestrator: Arc<SyncOrchestrator>,
    pub gossip_bridge: Arc<GossipBridge>,
}

impl SyncPayload {
    pub async fn new(
        read_provider: Arc<DatabaseReadProvider>,
        peer_manager: Arc<PeerManager>,
        engine: Arc<Engine>,
        chain_manager: Arc<dyn ChainManager>,
        sync_registry: Arc<SyncRegistry>,
        sync_service: Arc<SyncService>,
    ) -> Self {
        let processor = BlockProcessor::new(engine.clone());
        let sync_controller = Arc::new(SyncController::new(
            (*read_provider).clone(),
            peer_manager.registry.clone(),
            processor,
            chain_manager.clone(),
            sync_registry.clone(),
            engine.mempool.clone(),
        ));

        let sync_orchestrator = Arc::new(SyncOrchestrator::new(sync_registry.clone(), sync_controller.clone() as Arc<dyn SyncProvider>));

        sync_service.set_provider(sync_controller.clone()).await;

        let gossip_bridge = Arc::new(GossipBridge::new(sync_service));

        Self {
            sync_controller,
            sync_orchestrator,
            gossip_bridge,
        }
    }

    pub fn start(
        &self,
        engine: Arc<Engine>,
        chain_manager: Arc<dyn ChainManager>,
        sync_service: Arc<SyncService>,
    ) -> Vec<JoinHandle<()>> {
        let mut tasks = Vec::new();

        // 1. Start GossipBridge
        let gb_task = self.gossip_bridge.clone();
        let gb_event_rx = engine.event_tx.subscribe();
        tasks.push(tokio::spawn(async move {
            gb_task.run(gb_event_rx).await;
        }));

        // 2. Start GossipService
        let gossip_rx = sync_service.gossip_rx();
        let gossip_service = GossipService::new(
            engine.clone(),
            chain_manager.clone(),
            self.sync_orchestrator.registry.clone(),
            gossip_rx,
        );
        tasks.push(tokio::spawn(async move {
            gossip_service.start().await;
        }));

        // 2. Start SyncController
        let sc_task = self.sync_controller.clone();
        tasks.push(tokio::spawn(async move {
            sc_task.start().await;
        }));

        // 3. Start SyncOrchestrator
        let so_task = self.sync_orchestrator.clone();
        tasks.push(tokio::spawn(async move {
            so_task.run().await;
        }));

        tasks
    }
}
