use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use wasix_eth_types::{Address, SignerRecoverable, ConsensusTransaction, B256, U256, Transaction, Blob, Bytes48, TxPooledEnvelope, Bytes};
use wasix_eth_utils::{debug, info, metrics::{MEMPOOL_SIZE, MEMPOOL_REJECTED_TRANSACTIONS}};

#[derive(Debug, Default, Clone)]
pub struct Mempool {
    pub inner: Arc<RwLock<MempoolInner>>,
}

impl Mempool {
    /// Create a new mempool with the given base fee.
    pub fn new(base_fee: U256) -> Self {
        Self {
            inner: Arc::new(RwLock::new(MempoolInner {
                pending_transactions: HashMap::new(),
                queued_transactions: HashMap::new(),
                pooled_envelopes: HashMap::new(),
                pooled_bytes: HashMap::new(),
                blobs: HashMap::new(),
                base_fee,
            })),
        }
    }

    pub fn add_transaction_no_nonce_sync(&self, tx: Transaction) -> bool {
        if let Ok(mut inner) = self.inner.try_write() {
            inner.add_transaction(tx, 0)
        } else {
            false
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MempoolInner {
    /// Transactions ready for inclusion (no nonce gaps, sufficient balance).
    pub pending_transactions: HashMap<Address, VecDeque<Transaction>>,
    /// Transactions waiting for a previous nonce to arrive.
    pub queued_transactions: HashMap<Address, VecDeque<Transaction>>,
    /// Original pooled envelopes for re-serving over P2P.
    pub pooled_envelopes: HashMap<B256, TxPooledEnvelope>,
    /// Original pooled bytes as received over RPC (wire-identical replay).
    pub pooled_bytes: HashMap<B256, Bytes>,
    /// Blobs, commitments and proofs associated with EIP-4844 transactions, indexed by versioned hash.
    pub blobs: HashMap<B256, (Blob, Bytes48, Bytes48)>,
    /// Current base fee for transactions in the mempool.
    pub base_fee: U256,
}

/// Metadata for a transaction in the mempool.
//TODO
#[derive(Debug, Clone)]
pub struct TxMetadata {
    pub arrival_time: u64,
    pub effective_tip: u128,
}

impl MempoolInner {
    /// Add a transaction to the mempool, considering its nonce and the current state nonce.
    /// Returns true if the transaction was newly added or replaced an existing one.
    pub fn add_transaction(&mut self, tx: Transaction, current_nonce: u64) -> bool {
        let hash = tx.hash();
        let from = tx.recover_signer().unwrap_or_default();
        let tx_nonce = tx.nonce();

        if tx_nonce < current_nonce {
            debug!("[Mempool] Rejecting transaction {:?} (nonce: {}) because it is below current nonce ({})", hash, tx_nonce, current_nonce);
            MEMPOOL_REJECTED_TRANSACTIONS.inc();
            return false;
        }

        // Potential addition: check balance against current state
        // This would require passing storage or balance to this method.

        // Determine if it should be pending or queued based on existing transactions from this sender
        let next_pending_nonce = self.pending_transactions.get(&from)
            .and_then(|q| q.back().map(|t| t.nonce() + 1))
            .unwrap_or(current_nonce);

        let added = if tx_nonce <= next_pending_nonce {
            // It either fills the next expected nonce or it's a replacement (handled in insert_into_queue)
            self.insert_into_queue(from, tx, true)
        } else {
            // There's a gap between pending transactions and this one
            self.insert_into_queue(from, tx, false)
        };

        if added {
            MEMPOOL_SIZE.set(self.len() as f64);
        } else {
            MEMPOOL_REJECTED_TRANSACTIONS.inc();
        }

        added
    }


    /// Internal helper to insert a transaction into pending or queued maps.
    fn insert_into_queue(&mut self, from: Address, tx: Transaction, is_pending: bool) -> bool {
        let hash = tx.hash();
        let nonce = tx.nonce();
        
        // Enforce capacity before adding
        self.enforce_capacity(5000); // MAX_MEMPOOL_SIZE

        let queue = if is_pending {
            self.pending_transactions.entry(from).or_insert_with(VecDeque::new)
        } else {
            self.queued_transactions.entry(from).or_insert_with(VecDeque::new)
        };

        // Insert in nonce order
        let pos = queue.binary_search_by_key(&nonce, |t| t.nonce())
            .unwrap_or_else(|e| e);

        // Replace-By-Fee (RBF) logic: 10% threshold
        let gas_price = tx.gas_price().unwrap_or_default();
        if pos < queue.len() && queue[pos].nonce() == nonce {
            let old_gas_price = queue[pos].gas_price().unwrap_or_default();
            // 10% increase requirement: new >= old * 1.1
            let min_replacement_price = old_gas_price + (old_gas_price / 10);
            
            if gas_price >= min_replacement_price {
                info!("[Mempool] Replacing transaction {:?} (nonce: {}) with {:?} (price: {} -> {})", 
                    queue[pos].hash(), nonce, hash, old_gas_price, gas_price);
                queue[pos] = tx;
                return true;
            }
            info!("[Mempool] Rejecting replacement for {:?} (nonce: {}): gas price bump too low ({} < {})", 
                hash, nonce, gas_price, min_replacement_price);
            return false;
        } else {
            info!("[Mempool] Adding transaction {:?} (nonce: {}) to {} pool", 
                hash, nonce, if is_pending { "pending" } else { "queued" });
            queue.insert(pos, tx);
            
            // If we added to pending, we might be able to promote queued transactions
            if is_pending {
                self.promote_queued(from, nonce + 1);
            }
            
            true
        }
    }

    /// Move transactions from queued to pending if the nonce gap is filled.
    pub fn promote_queued(&mut self, address: Address, next_expected_nonce: u64) {
        let mut next_nonce = next_expected_nonce;
        
        while let Some(queued_queue) = self.queued_transactions.get_mut(&address) {
            // Since it's sorted, check the first one
            if let Some(pos) = queued_queue.iter().position(|tx| tx.nonce() == next_nonce) {
                let tx = queued_queue.remove(pos).unwrap();
                info!("[Mempool] Promoting transaction {:?} (nonce: {}) for {} to pending", tx.hash(), tx.nonce(), address);
                
                let pending_queue = self.pending_transactions.entry(address).or_insert_with(VecDeque::new);
                let p_pos = pending_queue.binary_search_by_key(&tx.nonce(), |t| t.nonce())
                    .unwrap_or_else(|e| e);
                pending_queue.insert(p_pos, tx);
                
                next_nonce += 1;
            } else {
                break;
            }
        }
        
        // Cleanup if empty
        if let Some(q) = self.queued_transactions.get(&address) {
            if q.is_empty() {
                self.queued_transactions.remove(&address);
            }
        }
    }

    /// Get all transactions currently in the mempool (both pending and queued).
    pub fn get_all_transactions(&self) -> Vec<Transaction> {
        let mut all = self.pending_transactions
            .values()
            .flat_map(|q: &VecDeque<Transaction>| q.iter().cloned())
            .collect::<Vec<_>>();
        
        all.extend(self.queued_transactions
            .values()
            .flat_map(|q: &VecDeque<Transaction>| q.iter().cloned()));
            
        all
    }

    /// Clear the mempool.
    pub fn clear(&mut self) {
        self.pending_transactions.clear();
        self.queued_transactions.clear();
        MEMPOOL_SIZE.set(0.0);
    }

    /// Remove transactions that have been included in a block.
    pub fn remove_transactions(&mut self, tx_hashes: &[B256]) {
        if !tx_hashes.is_empty() {
            info!("[Mempool] Removing {} transactions from mempool", tx_hashes.len());
        }
        for queue in self.pending_transactions.values_mut() {
            queue.retain(|tx| !tx_hashes.contains(&tx.hash()));
        }
        for queue in self.queued_transactions.values_mut() {
            queue.retain(|tx| !tx_hashes.contains(&tx.hash()));
        }
        // Clean up empty queues
        self.pending_transactions.retain(|_, queue| !queue.is_empty());
        self.queued_transactions.retain(|_, queue| !queue.is_empty());
        
        MEMPOOL_SIZE.set(self.len() as f64);
    }

    /// Peek at best transactions from the mempool for block building without removing them.
    /// Prioritizes by effective tip and respects nonce order, gas limit and blob gas limit (Cancun).
    pub fn peek_best_transactions(&self, target_gas_limit: u64, base_fee: U256, blob_base_fee: Option<U256>, max_blobs_per_block: Option<u32>) -> Vec<Transaction> {
        let mut result = Vec::new();
        let mut current_gas: u64 = 0;
        let mut current_blobs: u32 = 0;
        let max_blobs = max_blobs_per_block.unwrap_or(6);
        
        // We use a simplified version of the greedy algorithm.
        let mut copy = self.pending_transactions.clone();
        
        while current_gas < target_gas_limit {
            let mut best_sender: Option<Address> = None;
            let mut best_tip: u128 = 0;
            let mut best_blobs: u32 = 0;

            for (address, queue) in &copy {
                if let Some(tx) = queue.front() {
                    let tip = Self::calculate_effective_tip(tx, base_fee);
                    // Skip transactions that can't pay the base fee
                    if tip == 0 && tx.max_fee_per_gas() < base_fee.to::<u128>() {
                        continue;
                    }
                    
                    // Skip EIP-4844 transactions that can't pay the blob base fee
                    if let (Some(blob_fee), Transaction::Eip4844(s)) = (blob_base_fee, tx) {
                        if s.max_fee_per_blob_gas().unwrap_or_default() < blob_fee.to::<u128>() {
                            continue;
                        }
                    }

                    let tx_blobs = if let Transaction::Eip4844(signed_tx) = tx {
                        signed_tx.tx().blob_versioned_hashes().map(|h| h.len()).unwrap_or(0) as u32
                    } else {
                        0
                    };

                    let fits = current_gas + tx.gas_limit() <= target_gas_limit && current_blobs + tx_blobs <= max_blobs;

                    if fits {
                        if tip > best_tip {
                            best_tip = tip;
                            best_sender = Some(*address);
                            best_blobs = tx_blobs;
                        } else if tip == best_tip {
                            // If tips are equal, prioritize the transaction that uses more blobs
                            // to help reach the target/max blob count.
                            if tx_blobs > best_blobs {
                                best_sender = Some(*address);
                                best_blobs = tx_blobs;
                            } else if best_sender.is_none() {
                                best_sender = Some(*address);
                                best_blobs = tx_blobs;
                            }
                        }
                    }
                }
            }

            if let Some(sender) = best_sender {
                let queue = copy.get_mut(&sender).unwrap();
                let tx = queue.pop_front().unwrap();
                
                let tx_gas = tx.gas_limit();
                let tx_blobs = if let Transaction::Eip4844(signed_tx) = &tx {
                    signed_tx.tx().blob_versioned_hashes().map(|h| h.len()).unwrap_or(0) as u32
                } else {
                    0
                };

                if current_gas + tx_gas <= target_gas_limit && current_blobs + tx_blobs <= max_blobs {
                    // Check if all blobs are present if it's an EIP-4844 transaction
                    let mut all_blobs_present = true;
                    if let Transaction::Eip4844(signed_tx) = &tx {
                        if let Some(hashes) = signed_tx.tx().blob_versioned_hashes() {
                            for hash in hashes {
                                if !self.blobs.contains_key(hash) {
                                    debug!("[Mempool] Missing blob {:?} for transaction {:?}, skipping", hash, tx.hash());
                                    all_blobs_present = false;
                                    break;
                                }
                            }
                        }
                    }

                    if all_blobs_present {
                        current_gas += tx_gas;
                        current_blobs += tx_blobs;
                        result.push(tx);
                    } else {
                        // If blobs are missing, we skip this transaction and its sender
                        copy.remove(&sender);
                    }
                } else {
                    // If it doesn't fit due to gas or blobs, we skip this sender for this block
                    // but we could technically try the next transaction from this sender if it's not blob-heavy?
                    // Actually no, because we must respect nonce order. If this transaction doesn't fit,
                    // no subsequent transaction from this sender can be included.
                    // IMPORTANT: We remove the sender from 'copy' but NOT from the original mempool, 
                    // allowing us to continue searching other senders in the next iteration of 'while'.
                    copy.remove(&sender);
                }
                
                if let Some(q) = copy.get(&sender) {
                    if q.is_empty() {
                        copy.remove(&sender);
                    }
                }
            } else {
                break;
            }
        }
        
        result
    }

    /// Peek at N transactions from the mempool for block building without removing them.
    pub fn peek_transactions(&self, n: usize) -> Vec<Transaction> {
        let mut result = Vec::with_capacity(n);
        let mut copy = self.clone();
        
        while result.len() < n {
            let mut best_sender: Option<Address> = None;
            let mut best_gas_price: u128 = 0;

            for (address, queue) in &copy.pending_transactions {
                if let Some(tx) = queue.front() {
                    let gas_price = tx.gas_price().unwrap_or_default();
                    if gas_price > best_gas_price {
                        best_gas_price = gas_price;
                        best_sender = Some(*address);
                    }
                }
            }

            if let Some(sender) = best_sender {
                if let Some(queue) = copy.pending_transactions.get_mut(&sender) {
                    if let Some(tx) = queue.pop_front() {
                        result.push(tx);
                    }
                    if queue.is_empty() {
                        copy.pending_transactions.remove(&sender);
                    }
                }
            } else {
                break;
            }
        }
        
        result
    }

    /// Pop N transactions from the mempool for inclusion in a block.
    /// Transactions are prioritized by effective tip while strictly respecting nonce order for each sender.
    pub fn pop_transactions(&mut self, n: usize) -> Vec<Transaction> {
        let mut result = Vec::with_capacity(n);
        
        while result.len() < n {
            let mut best_sender: Option<Address> = None;
            let mut best_tip: u128 = 0;

            for (address, queue) in &self.pending_transactions {
                if let Some(tx) = queue.front() {
                    let tip = Self::calculate_effective_tip(tx, self.base_fee);
                    if tip > best_tip || (tip == best_tip && best_sender.is_none()) {
                        best_tip = tip;
                        best_sender = Some(*address);
                    }
                }
            }

            if let Some(sender) = best_sender {
                if let Some(queue) = self.pending_transactions.get_mut(&sender) {
                    if let Some(tx) = queue.pop_front() {
                        result.push(tx);
                    }
                    if queue.is_empty() {
                        self.pending_transactions.remove(&sender);
                    }
                }
            } else {
                break;
            }
        }
        
        if !result.is_empty() {
            MEMPOOL_SIZE.set(self.len() as f64);
        }

        result
    }

    /// Calculate the effective tip for a transaction based on the current base fee.
    pub fn calculate_effective_tip(tx: &Transaction, base_fee: U256) -> u128 {
        let base_fee = base_fee.to::<u128>();
        let max_fee = tx.max_fee_per_gas();
        if max_fee < base_fee {
            0
        } else {
            let max_tip = max_fee - base_fee;
            max_tip.min(tx.max_priority_fee_per_gas().unwrap_or_default())
        }
    }

    /// Evict transactions with the lowest effective tip when the mempool is over capacity.
    pub fn enforce_capacity(&mut self, max_size: usize) {
        while self.len() > max_size {
            let mut worst_sender: Option<(Address, bool)> = None; // (address, is_pending)
            let mut worst_tip: u128 = u128::MAX;

            // Prioritize evicting from queued first
            for (address, queue) in &self.queued_transactions {
                if let Some(tx) = queue.back() {
                    let tip = Self::calculate_effective_tip(tx, self.base_fee);
                    if tip < worst_tip {
                        worst_tip = tip;
                        worst_sender = Some((*address, false));
                    }
                }
            }

            // If still over capacity and queued is empty or we have even worse pending
            if worst_sender.is_none() || self.queued_transactions.is_empty() {
                for (address, queue) in &self.pending_transactions {
                    if let Some(tx) = queue.back() {
                        let tip = Self::calculate_effective_tip(tx, self.base_fee);
                        if tip < worst_tip {
                            worst_tip = tip;
                            worst_sender = Some((*address, true));
                        }
                    }
                }
            }

            if let Some((address, is_pending)) = worst_sender {
                let queue = if is_pending {
                    self.pending_transactions.get_mut(&address)
                } else {
                    self.queued_transactions.get_mut(&address)
                };

                if let Some(q) = queue {
                    if let Some(tx) = q.pop_back() {
                        info!("[Mempool] Evicting transaction {:?} (nonce: {}) from {} pool due to capacity (tip: {})", 
                            tx.hash(), tx.nonce(), if is_pending { "pending" } else { "queued" }, worst_tip);
                    }
                    if q.is_empty() {
                        if is_pending {
                            self.pending_transactions.remove(&address);
                        } else {
                            self.queued_transactions.remove(&address);
                        }
                    }
                }
            } else {
                break;
            }
        }
    }

    /// Get the total count of transactions in the mempool (pending + queued).
    pub fn len(&self) -> usize {
        self.pending_transactions.values().map(|q: &VecDeque<Transaction>| q.len()).sum::<usize>() +
        self.queued_transactions.values().map(|q: &VecDeque<Transaction>| q.len()).sum::<usize>()
    }

    /// Check if the mempool is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use wasix_eth_types::{SignableTransaction, Signature, TxLegacy, Signed};
    use crate::mempool::mempool_provider::MempoolProvider;

    #[tokio::test]
    async fn test_peek_best_transactions_priority() {
        let mempool = Mempool::new(U256::from(100));

        // Tx from addr1: tip 50, gas 21000
        let tx1 = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 150,
                gas_limit: 21000,
                to: Address::ZERO.into(),
                value: U256::ZERO,
                input: Default::default(),
                chain_id: None,
            },
            Signature::test_signature(),
            B256::ZERO,
        ));

        // Tx from addr2: tip 100, gas 21000
        let tx2 = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 200,
                gas_limit: 21000,
                to: Address::ZERO.into(),
                value: U256::ZERO,
                input: Default::default(),
                chain_id: None,
            },
            Signature::test_signature(),
            B256::repeat_byte(1),
        ));

        mempool.inner.write().await.add_transaction(tx1.clone(), 0);
        mempool.inner.write().await.add_transaction(tx2.clone(), 0);

        // Peek with gas limit enough for both
        let best = mempool.peek_best_transactions(50000, U256::from(100), None, None).await;
        assert_eq!(best.len(), 2);
        assert_eq!(best[0].hash(), tx2.hash()); // tx2 has higher tip
        assert_eq!(best[1].hash(), tx1.hash());

        // Peek with gas limit enough for only one
        let best_one = mempool.peek_best_transactions(30000, U256::from(100), None, None).await;
        assert_eq!(best_one.len(), 1);
        assert_eq!(best_one[0].hash(), tx2.hash());
    }

    #[tokio::test]
    async fn test_peek_best_transactions_nonce_order() {
        let mempool = Mempool::new(U256::from(100));

        // Tx1: nonce 0, tip 100
        let tx1 = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 200,
                gas_limit: 21000,
                to: Address::ZERO.into(),
                value: U256::ZERO,
                input: Default::default(),
                chain_id: None,
            },
            Signature::test_signature(),
            B256::ZERO,
        ));

        // Tx2: nonce 1, tip 200
        let tx2 = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 1,
                gas_price: 300,
                gas_limit: 21000,
                to: Address::ZERO.into(),
                value: U256::ZERO,
                input: Default::default(),
                chain_id: None,
            },
            Signature::test_signature(),
            B256::repeat_byte(1),
        ));

        let mut inner = mempool.inner.write().await;
        let from = tx1.recover_signer().unwrap_or_default();
        inner.add_transaction(tx1.clone(), 0);
        inner.pending_transactions.entry(from).or_default().push_back(tx2.clone());
        drop(inner);

        let best = mempool.peek_best_transactions(100000, U256::from(100), None, None).await;
        assert_eq!(best.len(), 2);
        assert_eq!(best[0].hash(), tx1.hash()); // Nonce 0 must come first even if Tx2 has higher tip
        assert_eq!(best[1].hash(), tx2.hash());
    }

    #[tokio::test]
    async fn test_mempool_new() {
        let mempool = Mempool::new(U256::from(100));
        assert_eq!(mempool.inner.read().await.base_fee, U256::from(100));
        assert!(mempool.inner.read().await.len() == 0);
    }

    #[tokio::test]
    async fn test_add_transaction_nonce_low() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 5,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        
        // current_nonce = 10, tx_nonce = 5 -> should be rejected (false)
        assert!(!mempool.inner.write().await.add_transaction(tx, 10));
        assert!(mempool.inner.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_add_transaction_pending() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 10,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        
        // current_nonce = 10, tx_nonce = 10 -> should be pending
        assert!(mempool.inner.write().await.add_transaction(tx, 10));
        assert_eq!(mempool.inner.read().await.len(), 1);
        assert_eq!(mempool.inner.read().await.pending_transactions.len(), 1);
    }

    #[tokio::test]
    async fn test_add_transaction_queued() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 15,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        
        // current_nonce = 10, tx_nonce = 15 -> should be queued
        assert!(mempool.inner.write().await.add_transaction(tx, 10));
        assert_eq!(mempool.inner.read().await.len(), 1);
        assert_eq!(mempool.inner.read().await.queued_transactions.len(), 1);
    }

    #[tokio::test]
    async fn test_promote_queued() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 10,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        let addr = tx.recover_signer().unwrap();
        
        {
            let mut inner = mempool.inner.write().await;
            inner.queued_transactions.entry(addr).or_default().push_back(tx);
            assert_eq!(inner.queued_transactions.len(), 1);
            
            inner.promote_queued(addr, 10);
            assert_eq!(inner.queued_transactions.len(), 0);
            assert_eq!(inner.pending_transactions.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_remove_transactions() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 10,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        let hash = *tx.hash();
        
        mempool.inner.write().await.add_transaction(tx, 10);
        assert_eq!(mempool.inner.read().await.len(), 1);
        
        mempool.inner.write().await.remove_transactions(&[hash]);
        assert_eq!(mempool.inner.read().await.len(), 0);
    }
}
