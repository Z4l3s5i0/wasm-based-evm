use std::sync::Arc;
use tokio::sync::broadcast;
use wasix_eth_utils::info;
use crate::{EngineEvent, GossipProvider};

pub struct GossipBridge {
    gossip: Arc<dyn GossipProvider>,
}

impl GossipBridge {
    pub fn new(gossip: Arc<dyn GossipProvider>) -> Self {
        Self { gossip }
    }

    pub async fn run(&self, mut event_rx: broadcast::Receiver<EngineEvent>) {
        info!("[GossipBridge] Starting Engine-to-P2P event bridge");
        while let Ok(event) = event_rx.recv().await {
            match event {
                EngineEvent::NewTransaction(tx) => self.gossip.broadcast_transaction(&tx).await,
                EngineEvent::NewBlock(block) => self.gossip.broadcast_block(&block).await,
            }
        }
    }
}