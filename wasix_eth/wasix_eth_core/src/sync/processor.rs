use crate::Engine;
use alloy_primitives::B256;
use std::sync::Arc;
use wasix_eth_types::{Block, Transaction};
use wasix_eth_utils::info;

#[derive(Clone)]
pub struct BlockProcessor {
    pub engine: Arc<Engine>,
}

impl BlockProcessor {
    pub fn new(
        engine: Arc<Engine>,
    ) -> Self {
        Self {
            engine,
        }
    }

    pub async fn process_block(&self, block: Block<Transaction>) -> anyhow::Result<()> {
        let block_hash = block.header.hash_slow();
        match self.engine.import_block(block).await {
            Ok(_) => {
                info!("[BlockProcessor] import_block successful for hash {}", block_hash);
                Ok(())
            }
            Err(e) => {
                Err(anyhow::anyhow!("Import failed for {}: {}", block_hash, e))
            }
        }
    }

    pub async fn process_transaction(&self, tx: Transaction) -> anyhow::Result<()> {
        self.engine.rpc_engine.submit_transaction(tx).await
            .map_err(|e| anyhow::anyhow!("Failed to submit transaction: {:?}", e))?;
        Ok(())
    }

    pub async fn process_pooled_transaction(&self, tx: wasix_eth_types::TxPooledEnvelope) -> anyhow::Result<()> {
        // Use alloy_rlp::Encodable specifically
        use alloy_rlp::Encodable;
        let mut data = Vec::new();
        tx.encode(&mut data);
        
        // Pass the already decoded tx to import_pooled_transaction
        self.engine.rpc_engine.import_pooled_transaction(tx, data).await
            .map_err(|e| anyhow::anyhow!("Failed to import pooled transaction: {:?}", e))?;
        Ok(())
    }

    pub async fn invalidate_descendants(&self, hash: B256) {
        self.engine.invalidate_descendants(hash).await;
    }
}


