use std::sync::Arc;
use tokio::sync::RwLock;
use crate::storage::storage::InMemoryStorage;
use crate::executor::Executor;
use alloy_consensus::Block as ConsensusBlock;
use alloy_consensus::TxEnvelope as Transaction;
use crate::info;

pub struct BlockProcessor {
    storage: Arc<RwLock<InMemoryStorage>>,
    executor: Arc<Executor>,
}

impl BlockProcessor {
    pub fn new(storage: Arc<RwLock<InMemoryStorage>>, executor: Arc<Executor>) -> Self {
        Self { storage, executor }
    }

    pub async fn process_block(&self, block: ConsensusBlock<Transaction>) -> Result<(), Box<dyn std::error::Error>> {
        let block_num = block.header.number;
        let mut storage_write = self.storage.write().await;
        
        match self.executor.execute_block(&mut storage_write, block.body.transactions.clone(), block) {
            Ok(_) => {
                info!("[Processor] Successfully executed and stored block {}", block_num);
                Ok(())
            }
            Err(e) => {
                Err(format!("Block execution failed for block {}: {}", block_num, e).into())
            }
        }
    }
}
