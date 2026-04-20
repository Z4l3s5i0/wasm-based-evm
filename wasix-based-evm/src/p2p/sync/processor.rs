use std::sync::Arc;
use tokio::sync::RwLock;
use crate::storage::storage::InMemoryStorage;
use crate::executor::Executor;
use crate::mempool::Mempool;
use alloy_consensus::Block as ConsensusBlock;
use alloy_consensus::TxEnvelope as Transaction;
use alloy_primitives::U256;
use crate::info;

pub struct BlockProcessor {
    storage: Arc<RwLock<InMemoryStorage>>,
    executor: Arc<Executor>,
    mempool: Arc<RwLock<Mempool>>,
}

impl BlockProcessor {
    pub fn new(storage: Arc<RwLock<InMemoryStorage>>, executor: Arc<Executor>, mempool: Arc<RwLock<Mempool>>) -> Self {
        Self { storage, executor, mempool }
    }

    pub async fn process_block(&self, block: ConsensusBlock<Transaction>) -> Result<(), Box<dyn std::error::Error>> {
        let block_num = block.header.number;
        let mut storage_write = self.storage.write().await;
        
        match self.executor.execute_block(&mut storage_write, block.body.transactions.clone(), block.clone()) {
            Ok(_) => {
                info!("[Processor] Successfully executed and stored block {}", block_num);
                let block_hash = block.header.hash_slow();
                storage_write.add_block(block.clone());
                storage_write.update_forkchoice(block_hash, Some(block_hash), Some(block_hash));
                
                // Update mempool after successful block processing
                let mut mempool_write = self.mempool.write().await;
                let new_base_fee = U256::from(block.header.base_fee_per_gas.unwrap_or_default());
                mempool_write.update_base_fee(new_base_fee, &*storage_write).await;
                
                Ok(())
            }
            Err(e) => {
                Err(format!("Block execution failed for block {}: {}", block_num, e).into())
            }
        }
    }
}
