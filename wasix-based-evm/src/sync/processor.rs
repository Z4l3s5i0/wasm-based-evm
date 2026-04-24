use std::sync::Arc;
use tokio::sync::RwLock;
use crate::storage::storage::InMemoryStorage;
use crate::mempool::Mempool;
use alloy_consensus::{Block, TxEnvelope as Transaction};
use alloy_primitives::U256;
use crate::evm::executor::Executor;
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

    pub async fn process_block(&self, block: Block<Transaction>) -> Result<(), Box<dyn std::error::Error>> {
        let block_num = block.header.number;
        let mut storage_write = self.storage.write().await;
        
        match self.executor.execute_block(&mut storage_write, block.body.transactions.clone(), block.clone()) {
            Ok(_) => {
                info!("[Processor] Successfully executed and stored block {}", block_num);
                let block_hash = block.header.hash_slow();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm::ev::EvmU256;

    #[tokio::test]
    async fn test_process_block_empty() {
        let storage = Arc::new(RwLock::new(InMemoryStorage::new(EvmU256::from(1))));
        let executor = Arc::new(Executor::new("native".to_string(), false));
        let mempool = Arc::new(RwLock::new(Mempool::new(U256::from(0))));
        let processor = BlockProcessor::new(storage.clone(), executor, mempool);
        
        let mut block: Block<Transaction> = Block::default();
        block.header.number = 1;
        // Need to set parent hash to genesis hash or it might fail if there's parent validation
        let genesis_hash = storage.read().await.head_block_hash;
        block.header.parent_hash = genesis_hash;
        
        let result = processor.process_block(block).await;
        assert!(result.is_ok());
        assert_eq!(storage.read().await.get_latest_block_number(), 1);
    }
}
