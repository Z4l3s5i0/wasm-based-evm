use crate::mempool::mempool_provider::MempoolProvider;
use std::sync::Arc;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::HeaderProvider;
use wasix_eth_types::{Block, BlockId, Transaction, B256, U256};
use wasix_eth_utils::{error, info};

pub struct MempoolListener {
    mempool: Arc<dyn MempoolProvider>,
    read_storage: DatabaseReadProvider,
}

impl MempoolListener {
    pub fn new(
        mempool: Arc<dyn MempoolProvider>,
        read_storage: DatabaseReadProvider,
    ) -> Self {
        Self {
            mempool,
            read_storage,
        }
    }

    pub async fn handle_canonical_block(&self, block: Block<Transaction>) {
        let base_fee = block.header.base_fee_per_gas.unwrap_or_default();
        let tx_hashes: Vec<B256> = block.body.transactions.iter().map(|tx| *tx.hash()).collect();

        // 1. Remove included transactions FIRST
        // This prevents them from being marked as "invalid" during revalidation
        if !tx_hashes.is_empty() {
            info!("[MempoolListener] Removing {} transactions included in canonical block #{}", tx_hashes.len(), block.header.number);
            self.mempool.remove_transactions(&tx_hashes).await;
        }

        // 2. Update base fee (which might trigger revalidation if it increased)
        self.mempool.update_base_fee(U256::from(base_fee), &self.read_storage).await;

        // 3. Always revalidate to promote queued transactions, even if base fee didn't increase
        // Note: update_base_fee might have already done this if fee increased, but revalidate is idempotent-ish
        self.mempool.revalidate(&self.read_storage).await;
    }
    
    pub async fn handle_reorg(&self, head_hash: B256) {
        info!("[MempoolListener] Handling reorg to block {:?}", head_hash);
        if let Ok(Some(header)) = self.read_storage.header(BlockId::Hash(head_hash.into())) {
            let base_fee = header.base_fee_per_gas.unwrap_or_default();
            self.mempool.update_base_fee(U256::from(base_fee), &self.read_storage).await;
            
            // On reorg, we should revalidate the entire mempool because many things might have changed
            self.mempool.revalidate(&self.read_storage).await;
        } else {
            error!("[MempoolListener] Failed to find header for new head {:?} after reorg", head_hash);
        }
    }
}
