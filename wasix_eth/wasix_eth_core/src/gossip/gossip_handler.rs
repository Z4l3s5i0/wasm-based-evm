use tokio::sync::mpsc::{Receiver, channel, Sender};
use std::sync::Arc;
use std::time::Duration;
use alloy_rlp::Decodable;
use wasix_eth_utils::{debug, error, info};
use wasix_eth_utils::metrics::GOSSIP_MESSAGES_RECEIVED;
use wasix_eth_types::{Block, ChainManager, Result, Transaction};
use wasix_eth_types::sync::SyncProvider;
use crate::sync::registry::SyncRegistry;
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
        
        let (block_tx, mut block_rx) = channel::<(Block<Transaction>, Vec<u8>)>(256);
        let (tx_tx, mut tx_rx) = channel::<Transaction>(1024);

        // Spawn Block processing task (High priority)
        let engine_blocks = self.engine.clone();
        let chain_blocks = self.chain.clone();
        let sync_registry_blocks = self.sync_registry.clone();
        tokio::spawn(async move {
            while let Some((block, _data)) = block_rx.recv().await {
                let block_hash = block.header.hash_slow();
                debug!("[Gossip] Processing block via gossip: {:?} (number {})", block_hash, block.header.number);
                
                let parent_exists = chain_blocks.has_block(block.header.parent_hash).await;
                
                if engine_blocks.import_block(block).await.is_ok() {
                    let has_block = chain_blocks.has_block(block_hash).await;
                    if !has_block && !parent_exists {
                        info!("[Gossip] New block received via gossip with unknown parent, triggering sync: {:?}", block_hash);
                        sync_registry_blocks.add_target(block_hash, None, chain_blocks.clone()).await;
                    }
                }
            }
        });

        // Spawn Transaction processing task (Lower priority)
        let engine_txs = self.engine.clone();
        tokio::spawn(async move {
            while let Some(tx) = tx_rx.recv().await {
                let tx_hash = *tx.hash();
                debug!("[Gossip] Processing transaction via gossip: {:?}", tx_hash);
                let _ = engine_txs.process_gossip_transactions(vec![tx]).await;
            }
        });

        tokio::time::sleep(Duration::from_millis(800)).await;
        while let Some(data) = self.gossip_rx.recv().await {
            GOSSIP_MESSAGES_RECEIVED.inc();
            if let Err(e) = self.route_message(data, &block_tx, &tx_tx).await {
                error!("[Gossip] Error routing gossip message: {}", e);
            }
        }
    }

    async fn route_message(
        &self, 
        data: Vec<u8>, 
        block_tx: &Sender<(Block<Transaction>, Vec<u8>)>,
        tx_tx: &Sender<Transaction>
    ) -> Result<(), Box<dyn std::error::Error>> {
        if data.len() > 10 * 1024 * 1024 {
            return Err("Gossip message too large".into());
        }

        let mut data_slice = data.as_slice();
        
        // Try decoding as a Block first
        if let Ok(block) = Block::decode(&mut data_slice) {
            let block: Block<Transaction> = block;
            
            // Basic block validation before queuing
            let (_head_hash, head_num) = self.chain.head_block().await;
            if block.header.number > head_num + 1024 {
                return Err(format!("Gossiped block {} too far in future (head {})", block.header.number, head_num).into());
            }

            if let Err(_) = block_tx.try_send((block, data)) {
                error!("[Gossip] Block processing queue full, dropping block");
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

        if let Err(_) = tx_tx.try_send(tx) {
            debug!("[Gossip] Transaction processing queue full, dropping tx");
        }

        Ok(())
    }
}

