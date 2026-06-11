use tokio::sync::mpsc::Receiver;
use std::sync::Arc;
use alloy_rlp::Decodable;
use wasix_eth_utils::{debug, error, info};
use wasix_eth_utils::metrics::GOSSIP_MESSAGES_RECEIVED;
use wasix_eth_types::{Block, Result, Transaction};
use crate::chain_manager::ChainManager;
use crate::gossip::engine_sink::EngineSink;
use crate::Engine;

pub struct GossipService {
    engine: Arc<Engine>,
    chain: Arc<dyn ChainManager>,
    gossip_rx: Receiver<Vec<u8>>,
}

impl GossipService {
    pub fn new(
        engine: Arc<Engine>,
        chain: Arc<dyn ChainManager>,
        gossip_rx: Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            engine,
            chain,
            gossip_rx,
        }
    }

    pub async fn start(mut self) {
        info!("[Gossip] Starting GossipHandler");
        while let Some(data) = self.gossip_rx.recv().await {
            GOSSIP_MESSAGES_RECEIVED.inc();
            match self.handle_message(data).await {
                Ok(_) => {},
                Err(e) => error!("[Gossip] Error handling gossip message: {}", e),
            }
        }
    }

    async fn handle_message(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        let mut data_slice = data.as_slice();
        
        // Try decoding as a Block first
        if let Ok(block) = Block::decode(&mut data_slice) {
            let block: Block<Transaction> = block;
            let block_hash = block.header.hash_slow();
            debug!("[Gossip] Received block via gossip: {:?} (number {})", block_hash, block.header.number);
            
            // Ingest block via sink
            if self.engine.ingest_block(block).await {
                // Check if we have it in sync layer
                let has_block = self.chain.has_block(block_hash).await;
                if !has_block {
                    // If we don't have it, trigger sync to process it and its ancestors
                    info!("[Gossip] New block received via gossip, triggering sync: {:?}", block_hash);
                    let chain = self.chain.clone();
                    tokio::spawn(async move {
                        let _ = chain.trigger_sync().await;
                    });
                }
            }
            return Ok(());
        }

        // Fallback to Transaction
        data_slice = data.as_slice();
        let tx = Transaction::decode(&mut data_slice)
            .map_err(|e| format!("Failed to decode gossip message as Block or Transaction: {}", e))?;

        let tx_hash = tx.hash().clone();
        debug!("[Gossip] Received transaction via gossip: {:?}", tx_hash);

        self.engine.ingest_transaction(tx).await;

        Ok(())
    }
}

