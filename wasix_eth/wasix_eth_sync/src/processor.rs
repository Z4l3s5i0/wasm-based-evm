use std::sync::Arc;
use alloy_primitives::B256;
use wasix_eth_core::Engine;
use wasix_eth_types::{Block, Transaction, SignerRecoverable, BlockId, BlockNumberOrTag};
use wasix_eth_utils::info;
use alloy_rlp::Encodable;

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
        
        match self.engine.import_block(block).await {
            Ok(_) => {
                info!("[Processor] Successfully executed and persisted block");
                Ok(())
            }
            Err(e) => {
                Err(anyhow::anyhow!("Import failed: {}", e))
            }
        }
    }

    pub async fn process_transaction(&self, tx: Transaction) -> anyhow::Result<()> {
        let from = tx.recover_signer().map_err(|e| anyhow::anyhow!("Signer recovery failed: {}", e))?;
        let nonce = self.engine.rpc_engine.get_transaction_count(from, BlockId::Number(BlockNumberOrTag::Latest)).await
            .unwrap_or(0);
            
        self.engine.mempool.add_transaction(tx, nonce).await;
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


