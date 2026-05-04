use crate::evm::executor::Executor;
use crate::info;
use crate::misc::metrics::CURRENT_HEAD_BLOCK;
use crate::storage::mempool::Mempool;
use crate::storage::traits::StateProvider;
use alloy_consensus::{Block, TxEnvelope as Transaction};
use alloy_primitives::U256;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct BlockProcessor {
    state_storage: Arc<dyn StateProvider>,
    executor: Arc<Executor>,
    mempool: Arc<RwLock<Mempool>>,
}

impl BlockProcessor {
    pub fn new(state_storage: Arc<dyn StateProvider>, executor: Arc<Executor>, mempool: Arc<RwLock<Mempool>>) -> Self {
        Self { state_storage, executor, mempool }
    }

    pub async fn process_block(&self, block: Block<Transaction>) -> Result<(), Box<dyn std::error::Error>> {
        let block_num = block.header.number;
        let mut storage_clone = self.state_storage.get_storage_clone().await?;
        
        match self.executor.execute_with_changeset(&mut storage_clone, block.body.transactions.clone(), block.clone()) {
            Ok((_, receipts, changeset)) => {
                CURRENT_HEAD_BLOCK.set(block_num as f64);
                
                info!("[Processor] Successfully executed and stored block {}", block_num);
                let block_hash = block.header.hash_slow();
                
                let writer = self.state_storage.writer();
                let withdrawals = block.body.withdrawals.clone().unwrap_or_default();
                writer.commit_block(block.clone(), receipts, changeset, withdrawals.into_iter().collect()).await?;
                writer.update_forkchoice(block_hash, Some(block_hash), Some(block_hash)).await?;
                
                // Update mempool after successful block processing
                let mut mempool_write = self.mempool.write().await;
                let new_base_fee = U256::from(block.header.base_fee_per_gas.unwrap_or_default());
                mempool_write.update_base_fee(new_base_fee, &storage_clone).await;
                
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
        use crate::storage::storage_provider::StorageProvider;
        let storage = Arc::new(RwLock::new(InMemoryStorage::new(EvmU256::from(1))));
        let executor = Arc::new(Executor::new());
        let mempool = Arc::new(RwLock::new(Mempool::new(U256::from(0))));
        let provider = Arc::new(StorageProvider::new(storage.clone(), mempool.clone(), executor.clone()));
        let processor = BlockProcessor::new(provider, executor, mempool);
        
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
