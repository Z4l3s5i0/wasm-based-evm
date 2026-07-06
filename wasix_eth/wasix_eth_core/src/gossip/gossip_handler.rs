use tokio::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;
use alloy_rlp::Decodable;
use wasix_eth_utils::{debug, error, info};
use wasix_eth_utils::metrics::GOSSIP_MESSAGES_RECEIVED;
use wasix_eth_types::{Block, ChainManager, Result, Transaction};
use crate::sync::registry::SyncRegistry;
use crate::gossip::engine_sink::EngineSink;
use crate::Engine;

pub struct GossipService {
    engine: Arc<Engine>,
    chain: Arc<dyn ChainManager>,
    sync_registry: Arc<SyncRegistry>,
    gossip_rx: Receiver<Vec<u8>>,
}

impl GossipService {
    pub fn new(
        engine: Arc<Engine>,
        chain: Arc<dyn ChainManager>,
        sync_registry: Arc<SyncRegistry>,
        gossip_rx: Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            engine,
            chain,
            sync_registry,
            gossip_rx,
        }
    }

    pub async fn start(mut self) {
        info!("[Gossip] Starting GossipHandler");
        tokio::time::sleep(Duration::from_millis(800)).await;
        while let Some(data) = self.gossip_rx.recv().await {
            GOSSIP_MESSAGES_RECEIVED.inc();
            match self.handle_message(data).await {
                Ok(_) => {},
                Err(e) => error!("[Gossip] Error handling gossip message: {}", e),
            }
        }
    }

    async fn handle_message(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        if data.len() > 10 * 1024 * 1024 {
            return Err("Gossip message too large".into());
        }

        let mut data_slice = data.as_slice();
        
        // Try decoding as a Block first
        if let Ok(block) = Block::decode(&mut data_slice) {
            let block: Block<Transaction> = block;
            let block_hash = block.header.hash_slow();

            // Basic block validation before ingestion
            let (_head_hash, head_num) = self.chain.head_block().await;
            
            // 1. Check if block number is too far in the future
            if block.header.number > head_num + 1024 {
                return Err(format!("Gossiped block {} too far in future (head {})", block.header.number, head_num).into());
            }

            // 2. Check if parent exists
            let parent_exists = self.chain.has_block(block.header.parent_hash).await;

            debug!("[Gossip] Received block via gossip: {:?} (number {})", block_hash, block.header.number);
            
            // Ingest block via sink
            if self.engine.ingest_block(block).await {
                // Check if we have it in sync layer
                let has_block = self.chain.has_block(block_hash).await;
                if !has_block && !parent_exists {
                    // If we don't have it and don't have its parent, trigger sync to process it and its ancestors
                    info!("[Gossip] New block received via gossip with unknown parent, triggering sync: {:?}", block_hash);
                    let registry = self.sync_registry.clone();
                    let chain = self.chain.clone();
                    tokio::spawn(async move {
                        registry.add_target(block_hash, None, chain).await;
                    });
                }
            }
            return Ok(());
        }

        // Fallback to Transaction
        if data.len() > 128 * 1024 {
            return Err("Transaction gossip message too large".into());
        }

        data_slice = data.as_slice();
        let tx = Transaction::decode(&mut data_slice)
            .map_err(|e| format!("Failed to decode gossip message as Block or Transaction: {}", e))?;

        let tx_hash = tx.hash().clone();
        debug!("[Gossip] Received transaction via gossip: {:?}", tx_hash);

        self.engine.ingest_transaction(tx).await;

        Ok(())
    }
}

