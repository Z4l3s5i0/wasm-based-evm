use crate::mempool::mempool_provider::MempoolProvider;
use std::sync::Arc;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::HeaderProvider;
use wasix_eth_types::{Block, BlockId, SignerRecoverable, Transaction, B256, U256};
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
        let mut affected_addresses = Vec::new();
        let mut tx_hashes = Vec::new();

        for tx in &block.body.transactions {
            tx_hashes.push(*tx.hash());
            if let Ok(sender) = tx.recover_signer() {
                affected_addresses.push(sender);
            }
        }

        // 1. Remove included transactions FIRST
        // This prevents them from being marked as "invalid" during revalidation
        if !tx_hashes.is_empty() {
            info!("[MempoolListener] Removing {} transactions included in canonical block #{}", tx_hashes.len(), block.header.number);
            self.mempool.remove_transactions(&tx_hashes).await;
        }

        // 2. Update base fee
        // This will trigger a FULL revalidation only if the base fee increased.
        // If it didn't increase, we still want to revalidate affected accounts to promote queued ones.
        let old_base_fee = self.mempool.base_fee().await;
        let new_base_fee = U256::from(base_fee);
        self.mempool.update_base_fee(new_base_fee, &self.read_storage).await;

        // 3. Revalidate affected accounts if full revalidation wasn't already triggered
        // A full revalidation is triggered if new_base_fee > old_base_fee.
        if new_base_fee <= old_base_fee && !affected_addresses.is_empty() {
            affected_addresses.sort();
            affected_addresses.dedup();
            self.mempool.revalidate(&self.read_storage, Some(affected_addresses)).await;
        }
    }

    pub async fn handle_reorg(&self, head_hash: B256) {
        info!("[MempoolListener] Handling reorg to block {:?}", head_hash);
        if let Ok(Some(header)) = self.read_storage.header(BlockId::Hash(head_hash.into())) {
            let base_fee = header.base_fee_per_gas.unwrap_or_default();
            self.mempool.update_base_fee(U256::from(base_fee), &self.read_storage).await;

            // On reorg, we should revalidate the entire mempool because many things might have changed
            self.mempool.revalidate(&self.read_storage, None).await;
        } else {
            error!("[MempoolListener] Failed to find header for new head {:?} after reorg", head_hash);
        }
    }
}
