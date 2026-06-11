use wasix_eth_types::{async_trait, Address, ConsensusTransaction, Transaction, B256, U256, Blob, Bytes48, TxPooledEnvelope, Bytes};
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::AccountProvider;
use crate::mempool::mempool::Mempool;
use wasix_eth_utils::info;

#[async_trait]
pub trait MempoolProvider: Send + Sync {
    async fn add_transaction(&self, tx: Transaction, current_nonce: u64) -> bool;
    async fn update_base_fee(&self, new_base_fee: U256, state: &DatabaseReadProvider);
    async fn revalidate(&self, state: &DatabaseReadProvider);
    async fn pop_transactions(&self, n: usize) -> Vec<Transaction>;
    async fn peek_transactions(&self, n: usize) -> Vec<Transaction>;
    async fn remove_transactions(&self, tx_hashes: &[B256]);
    async fn get_all_transactions(&self) -> Vec<Transaction>;
    async fn len(&self) -> usize;
    async fn is_empty(&self) -> bool;
    async fn clear(&self);
    async fn base_fee(&self) -> U256;
    async fn peek_best_transactions(&self, target_gas_limit: u64, base_fee: U256, blob_base_fee: Option<U256>, max_blobs_per_block: Option<u32>) -> Vec<Transaction>;
    async fn add_blob(&self, versioned_hash: B256, blob: Blob, commitment: Bytes48, proof: Bytes48);
    async fn get_blob(&self, versioned_hash: B256) -> Option<(Blob, Bytes48, Bytes48)>;
    async fn get_transaction(&self, hash: B256) -> Option<Transaction>;
    async fn add_pooled_envelope(&self, hash: B256, pooled: TxPooledEnvelope);
    async fn get_pooled_envelope(&self, hash: B256) -> Option<TxPooledEnvelope>;
    async fn add_pooled_bytes(&self, hash: B256, bytes: Bytes);
    async fn get_pooled_bytes(&self, hash: B256) -> Option<Bytes>;
}

#[async_trait]
impl MempoolProvider for Mempool {
    async fn add_transaction(&self, tx: Transaction, current_nonce: u64) -> bool {
        let mut inner = self.inner.write().await;
        inner.add_transaction(tx, current_nonce)
    }

    async fn update_base_fee(&self, new_base_fee: U256, state: &DatabaseReadProvider) {
        let (increased, old_fee) = {
            let mut inner = self.inner.write().await;
            let increased = new_base_fee > inner.base_fee;
            let old_fee = inner.base_fee;
            inner.base_fee = new_base_fee;
            (increased, old_fee)
        };

        if increased {
            info!("[Mempool] Base fee increased from {} to {}. Revalidating mempool...", old_fee, new_base_fee);
            self.revalidate(state).await;
        }
    }

    /// Revalidate the mempool against the latest state.
    /// Removes transactions that are no longer valid (e.g. nonce too low, insufficient balance).
    async fn revalidate(&self, state: &DatabaseReadProvider) {
        let (pending, queued, base_fee) = {
            let inner = self.inner.read().await;
            (inner.pending_transactions.clone(), inner.queued_transactions.clone(), inner.base_fee)
        };

        let mut to_remove = Vec::new();

        // Check pending transactions
        for (address, queue) in &pending {
            let (current_nonce, current_balance) = if let Ok(Some(acc)) = state.account(*address, None) {
                (acc.nonce, acc.balance)
            } else {
                (0, U256::ZERO)
            };

            for tx in queue {
                let tx_nonce = tx.nonce();
                // Check if transaction can pay the base fee
                let max_fee = tx.max_fee_per_gas();

                if max_fee < base_fee.to::<u128>() {
                    info!("[Mempool] Evicting transaction {:?} (nonce: {}) because max fee {} is below base fee {}", 
                        tx.hash(), tx_nonce, max_fee, base_fee);
                    to_remove.push(*tx.hash());
                    continue;
                }

                // A very simplified balance check: gas_limit * gas_price + value
                let gas_limit = tx.gas_limit() as u128;
                let gas_price = tx.gas_price().unwrap_or_default();
                let value = tx.value().to::<u128>();
                let required_balance = U256::from(gas_limit * gas_price + value);

                if tx_nonce < current_nonce || current_balance < required_balance {
                    to_remove.push(*tx.hash());
                }
            }
        }

        // Check queued transactions
        for (address, queue) in &queued {
            let (current_nonce, current_balance) = if let Ok(Some(acc)) = state.account(*address, None) {
                (acc.nonce, acc.balance)
            } else {
                (0, U256::ZERO)
            };

            for tx in queue {
                let tx_nonce = tx.nonce();
                // Check if transaction can pay the base fee
                let max_fee = tx.max_fee_per_gas();

                if max_fee < base_fee.to::<u128>() {
                    info!("[Mempool] Evicting transaction {:?} (nonce: {}) because max fee {} is below base fee {}", 
                        tx.hash(), tx_nonce, max_fee, base_fee);
                    to_remove.push(*tx.hash());
                    continue;
                }

                let gas_limit = tx.gas_limit() as u128;
                let gas_price = tx.gas_price().unwrap_or_default();
                let value = tx.value().to::<u128>();
                let required_balance = U256::from(gas_limit * gas_price + value);

                if tx_nonce < current_nonce || current_balance < required_balance {
                    to_remove.push(*tx.hash());
                }
            }
        }

        if !to_remove.is_empty() {
            info!("[Mempool] Removing {} invalid transactions during revalidation", to_remove.len());
            self.remove_transactions(&to_remove).await;
        }

        // Try to promote queued transactions in case some were removed or new ones can be moved
        let addresses: Vec<Address> = queued.keys().copied().collect();
        for address in addresses {
            let current_nonce = if let Ok(Some(acc)) = state.account(address, None) {
                acc.nonce
            } else {
                0
            };
            let mut inner = self.inner.write().await;
            inner.promote_queued(address, current_nonce);
        }
    }

    async fn pop_transactions(&self, n: usize) -> Vec<Transaction> {
        let mut inner = self.inner.write().await;
        inner.pop_transactions(n)
    }

    async fn peek_transactions(&self, n: usize) -> Vec<Transaction> {
        let inner = self.inner.read().await;
        inner.peek_transactions(n)
    }

    async fn remove_transactions(&self, tx_hashes: &[B256]) {
        let mut inner = self.inner.write().await;
        inner.remove_transactions(tx_hashes);
    }

    async fn get_all_transactions(&self) -> Vec<Transaction> {
        let inner = self.inner.read().await;
        inner.get_all_transactions()
    }

    async fn len(&self) -> usize {
        let inner = self.inner.read().await;
        inner.len()
    }

    async fn is_empty(&self) -> bool {
        let inner = self.inner.read().await;
        inner.is_empty()
    }

    async fn clear(&self) {
        let mut inner = self.inner.write().await;
        inner.clear();
    }

    async fn base_fee(&self) -> U256 {
        let inner = self.inner.read().await;
        inner.base_fee
    }

    async fn peek_best_transactions(&self, target_gas_limit: u64, base_fee: U256, blob_base_fee: Option<U256>, max_blobs_per_block: Option<u32>) -> Vec<Transaction> {
        let inner = self.inner.read().await;
        inner.peek_best_transactions(target_gas_limit, base_fee, blob_base_fee, max_blobs_per_block)
    }

    async fn add_blob(&self, versioned_hash: B256, blob: Blob, commitment: Bytes48, proof: Bytes48) {
        let mut inner = self.inner.write().await;
        inner.blobs.insert(versioned_hash, (blob, commitment, proof));
    }

    async fn get_blob(&self, versioned_hash: B256) -> Option<(Blob, Bytes48, Bytes48)> {
        let inner = self.inner.read().await;
        inner.blobs.get(&versioned_hash).cloned()
    }

    async fn get_transaction(&self, hash: B256) -> Option<Transaction> {
        let inner = self.inner.read().await;
        for queue in inner.pending_transactions.values() {
            if let Some(tx) = queue.iter().find(|tx| *tx.hash() == hash) {
                return Some(tx.clone());
            }
        }
        for queue in inner.queued_transactions.values() {
            if let Some(tx) = queue.iter().find(|tx| *tx.hash() == hash) {
                return Some(tx.clone());
            }
        }
        None
    }

    async fn add_pooled_envelope(&self, hash: B256, pooled: TxPooledEnvelope) {
        let mut inner = self.inner.write().await;
        inner.pooled_envelopes.insert(hash, pooled);
    }

    async fn get_pooled_envelope(&self, hash: B256) -> Option<TxPooledEnvelope> {
        let inner = self.inner.read().await;
        inner.pooled_envelopes.get(&hash).cloned()
    }

    async fn add_pooled_bytes(&self, hash: B256, bytes: Bytes) {
        let mut inner = self.inner.write().await;
        inner.pooled_bytes.insert(hash, bytes);
    }

    async fn get_pooled_bytes(&self, hash: B256) -> Option<Bytes> {
        let inner = self.inner.read().await;
        inner.pooled_bytes.get(&hash).cloned()
    }
}
pub struct NoopMempoolProvider;

#[async_trait]
impl MempoolProvider for NoopMempoolProvider {
    async fn add_transaction(&self, _tx: Transaction, _current_nonce: u64) -> bool { true }
    async fn update_base_fee(&self, _new_base_fee: U256, _state: &DatabaseReadProvider) {}
    async fn revalidate(&self, _state: &DatabaseReadProvider) {}
    async fn pop_transactions(&self, _n: usize) -> Vec<Transaction> { Vec::new() }
    async fn peek_transactions(&self, _n: usize) -> Vec<Transaction> { Vec::new() }
    async fn remove_transactions(&self, _tx_hashes: &[B256]) {}
    async fn get_all_transactions(&self) -> Vec<Transaction> { Vec::new() }
    async fn len(&self) -> usize { 0 }
    async fn is_empty(&self) -> bool { true }
    async fn clear(&self) {}
    async fn base_fee(&self) -> U256 { U256::ZERO }
    async fn peek_best_transactions(&self, _target_gas_limit: u64, _base_fee: U256, _blob_base_fee: Option<U256>, _max_blobs_per_block: Option<u32>) -> Vec<Transaction> { Vec::new() }
    async fn add_blob(&self, _versioned_hash: B256, _blob: Blob, _commitment: Bytes48, _proof: Bytes48) {}
    async fn get_blob(&self, _versioned_hash: B256) -> Option<(Blob, Bytes48, Bytes48)> { None }
    async fn get_transaction(&self, _hash: B256) -> Option<Transaction> { None }
    async fn add_pooled_envelope(&self, _hash: B256, _pooled: TxPooledEnvelope) {}
    async fn get_pooled_envelope(&self, _hash: B256) -> Option<TxPooledEnvelope> { None }
    async fn add_pooled_bytes(&self, _hash: B256, _bytes: Bytes) {}
    async fn get_pooled_bytes(&self, _hash: B256) -> Option<Bytes> { None }
}
