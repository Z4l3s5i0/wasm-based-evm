use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use wasix_eth_utils::info;
use crate::{EngineEvent, GossipProvider};
use std::collections::HashSet;
use wasix_eth_types::B256;

pub struct GossipBridge {
    gossip: Arc<dyn GossipProvider>,
    seen_hashes: Mutex<HashSet<B256>>,
}

impl GossipBridge {
    pub fn new(gossip: Arc<dyn GossipProvider>) -> Self {
        Self { 
            gossip,
            seen_hashes: Mutex::new(HashSet::with_capacity(2048)),
        }
    }

    pub async fn run(&self, mut event_rx: broadcast::Receiver<EngineEvent>) {
        info!("[GossipBridge] Starting Engine-to-P2P event bridge");
        while let Ok(event) = event_rx.recv().await {
            match event {
                EngineEvent::NewTransaction { tx, is_local } => {
                    let hash = *tx.hash();
                    let mut seen = self.seen_hashes.lock().await;
                    
                    if seen.contains(&hash) {
                        continue;
                    }

                    // For local transactions, we always broadcast.
                    // For remote transactions, we broadcast only if we haven't seen them before.
                    if is_local || !seen.contains(&hash) {
                        if seen.len() > 10000 {
                            seen.clear();
                        }
                        seen.insert(hash);
                        drop(seen);
                        
                        if is_local {
                            info!("[GossipBridge] Broadcasting local transaction {}", hash);
                        } else {
                            info!("[GossipBridge] Re-broadcasting remote transaction {}", hash);
                        }
                        self.gossip.broadcast_transaction(&tx).await;
                    }
                }
                EngineEvent::NewBlock { block, is_local } => {
                    let hash = block.header.hash_slow();
                    let mut seen = self.seen_hashes.lock().await;

                    if seen.contains(&hash) {
                        continue;
                    }

                    if is_local || !seen.contains(&hash) {
                        if seen.len() > 10000 {
                            seen.clear();
                        }
                        seen.insert(hash);
                        drop(seen);

                        if is_local {
                            info!("[GossipBridge] Broadcasting local block {} (hash: {:?})", block.header.number, hash);
                        } else {
                            info!("[GossipBridge] Re-broadcasting remote block {} (hash: {:?})", block.header.number, hash);
                        }
                        self.gossip.broadcast_block(&block).await;
                    }
                }
            }
        }
    }
}