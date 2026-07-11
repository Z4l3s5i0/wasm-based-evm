use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use dashmap::DashMap;
use wasix_eth_types::{Address, SignerRecoverable, ConsensusTransaction, B256, U256, Transaction, Blob, Bytes48, TxPooledEnvelope, Bytes};
use wasix_eth_utils::{debug, info, metrics::{MEMPOOL_SIZE, MEMPOOL_REJECTED_TRANSACTIONS}};

#[derive(Debug, Default, Clone)]
pub struct Mempool {
    pub inner: Arc<MempoolInner>,
}

impl Mempool {
    /// Create a new mempool with the given base fee.
    pub fn new(base_fee: U256) -> Self {
        Self {
            inner: Arc::new(MempoolInner {
                pending_transactions: DashMap::new(),
                queued_transactions: DashMap::new(),
                pooled_envelopes: DashMap::new(),
                pooled_bytes: DashMap::new(),
                blobs: DashMap::new(),
                base_fee: RwLock::new(base_fee),
            }),
        }
    }

    pub async fn add_transaction_no_nonce_sync(&self, tx: Transaction) -> bool {
        // We can't easily get current_nonce here without state, so we use 0 or let it be handled by add_transaction
        self.inner.add_transaction(tx, 0).await
    }

    /// Get the next expected nonce for an address, considering pending transactions in the mempool.
    /// If no pending transactions exist, returns the provided state nonce.
    pub async fn next_expected_nonce(&self, address: Address, state_nonce: u64) -> u64 {
        self.inner.pending_transactions.get(&address)
            .and_then(|q| q.back().map(|t| t.nonce() + 1))
            .unwrap_or(state_nonce)
    }
}

#[derive(Debug, Default)]
pub struct MempoolInner {
    /// Transactions ready for inclusion (no nonce gaps, sufficient balance).
    pub pending_transactions: DashMap<Address, VecDeque<Transaction>>,
    /// Transactions waiting for a previous nonce to arrive.
    pub queued_transactions: DashMap<Address, VecDeque<Transaction>>,
    /// Original pooled envelopes for re-serving over P2P.
    pub pooled_envelopes: DashMap<B256, TxPooledEnvelope>,
    /// Original pooled bytes as received over RPC (wire-identical replay).
    pub pooled_bytes: DashMap<B256, Bytes>,
    /// Blobs, commitments and proofs associated with EIP-4844 transactions, indexed by versioned hash.
    pub blobs: DashMap<B256, (Blob, Bytes48, Bytes48)>,
    /// Current base fee for transactions in the mempool.
    pub base_fee: RwLock<U256>,
}

/// Metadata for a transaction in the mempool.
#[derive(Debug, Clone)]
pub struct TxMetadata {
    pub arrival_time: u64,
    pub effective_tip: u128,
}

impl MempoolInner {
    /// Add a transaction to the mempool, considering its nonce and the current state nonce.
    /// Returns true if the transaction was newly added or replaced an existing one.
    pub async fn add_transaction(&self, tx: Transaction, current_nonce: u64) -> bool {
        let hash = tx.hash();
        let from = tx.recover_signer().unwrap_or_default();
        let tx_nonce = tx.nonce();

        if tx_nonce < current_nonce {
            debug!("[Mempool] Rejecting transaction {:?} (nonce: {}) because it is below current nonce ({})", hash, tx_nonce, current_nonce);
            MEMPOOL_REJECTED_TRANSACTIONS.inc();
            return false;
        }

        // Determine if it should be pending or queued based on existing transactions from this sender
        let next_pending_nonce = self.pending_transactions.get(&from)
            .and_then(|q| q.back().map(|t| t.nonce() + 1))
            .unwrap_or(current_nonce);

        let added = if tx_nonce <= next_pending_nonce {
            // It either fills the next expected nonce or it's a replacement (handled in insert_into_queue)
            self.insert_into_queue(from, tx, true).await
        } else {
            // There's a gap between pending transactions and this one
            self.insert_into_queue(from, tx, false).await
        };

        if added {
            MEMPOOL_SIZE.set(self.len() as f64);
        } else {
            MEMPOOL_REJECTED_TRANSACTIONS.inc();
        }

        added
    }


    /// Internal helper to insert a transaction into pending or queued maps.
    async fn insert_into_queue(&self, from: Address, tx: Transaction, is_pending: bool) -> bool {
        let hash = tx.hash();
        let nonce = tx.nonce();
        
        // Enforce capacity before adding
        self.enforce_capacity(5000).await; // MAX_MEMPOOL_SIZE

        let mut queue = if is_pending {
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
                debug!("[Mempool] Replacing transaction {:?} (nonce: {}) with {:?} (price: {} -> {})",
                    queue[pos].hash(), nonce, hash, old_gas_price, gas_price);
                queue[pos] = tx;
                return true;
            }
            debug!("[Mempool] Rejecting replacement for {:?} (nonce: {}): gas price bump too low ({} < {})",
                hash, nonce, gas_price, min_replacement_price);
            return false;
        } else {
            debug!("[Mempool] Adding transaction {:?} (nonce: {}) to {} pool",
                hash, nonce, if is_pending { "pending" } else { "queued" });
            queue.insert(pos, tx);
            
            // If we added to pending, we might be able to promote queued transactions
            if is_pending {
                // Optimization: only promote if we added the next sequential nonce
                // (or if it's the first one, which next_pending_nonce handled in add_transaction)
                drop(queue);
                self.promote_queued(from, nonce + 1);
            }
            
            true
        }
    }

    /// Move transactions from queued to pending if the nonce gap is filled.
    pub fn promote_queued(&self, address: Address, next_expected_nonce: u64) {
        let mut next_nonce = next_expected_nonce;
        
        loop {
            let tx = {
                let mut queued_queue = match self.queued_transactions.get_mut(&address) {
                    Some(q) => q,
                    None => break,
                };
                
                match queued_queue.binary_search_by_key(&next_nonce, |tx| tx.nonce()) {
                    Ok(pos) => queued_queue.remove(pos),
                    Err(_) => None,
                }
            };

            if let Some(tx) = tx {
                info!("[Mempool] Promoting transaction {:?} (nonce: {}) for {} to pending", tx.hash(), tx.nonce(), address);
                
                let mut pending_queue = self.pending_transactions.entry(address).or_insert_with(VecDeque::new);
                let p_pos = pending_queue.binary_search_by_key(&tx.nonce(), |t| t.nonce())
                    .unwrap_or_else(|e| e);
                pending_queue.insert(p_pos, tx);
                
                next_nonce += 1;
            } else {
                break;
            }
        }
        
        // Cleanup if empty
        self.queued_transactions.remove_if(&address, |_, q| q.is_empty());
    }

    /// Get all transactions currently in the mempool (both pending and queued).
    pub fn get_all_transactions(&self) -> Vec<Transaction> {
        let mut all = Vec::new();
        for r in self.pending_transactions.iter() {
            all.extend(r.value().iter().cloned());
        }
        for r in self.queued_transactions.iter() {
            all.extend(r.value().iter().cloned());
        }
        all
    }

    /// Clear the mempool.
    pub fn clear(&self) {
        self.pending_transactions.clear();
        self.queued_transactions.clear();
        MEMPOOL_SIZE.set(0.0);
    }

    /// Remove transactions that have been included in a block.
    pub fn remove_transactions(&self, tx_hashes: &[B256]) {
        if !tx_hashes.is_empty() {
            info!("[Mempool] Removing {} transactions from mempool", tx_hashes.len());
        }
        for mut r in self.pending_transactions.iter_mut() {
            r.value_mut().retain(|tx| !tx_hashes.contains(tx.hash()));
        }
        for mut r in self.queued_transactions.iter_mut() {
            r.value_mut().retain(|tx| !tx_hashes.contains(tx.hash()));
        }
        // Clean up empty queues
        self.pending_transactions.retain(|_, queue| !queue.is_empty());
        self.queued_transactions.retain(|_, queue| !queue.is_empty());
        
        MEMPOOL_SIZE.set(self.len() as f64);
    }

    /// Peek at best transactions from the mempool for block building without removing them.
    /// Prioritizes by effective tip and respects nonce order, gas limit and blob gas limit (Cancun).
    pub async fn peek_best_transactions(&self, target_gas_limit: u64, base_fee: U256, blob_base_fee: Option<U256>, max_blobs_per_block: Option<u32>) -> Vec<Transaction> {
        let mut result = Vec::new();
        let mut current_gas: u64 = 0;
        let mut current_blobs: u32 = 0;
        let max_blobs = max_blobs_per_block.unwrap_or(6);

        // First, try to promote any queued transactions for all senders who already have pending transactions
        // or whose next expected nonce is in queued.
        let queued_senders: Vec<Address> = self.queued_transactions.iter().map(|r| *r.key()).collect();
        for sender in queued_senders {
            if let Some(pending_queue) = self.pending_transactions.get(&sender) {
                if let Some(last_tx) = pending_queue.back() {
                    let next_nonce = last_tx.nonce() + 1;
                    drop(pending_queue);
                    self.promote_queued(sender, next_nonce);
                }
            }
        }
        
        let mut sender_indices: HashMap<Address, usize> = HashMap::new();
        let mut active_senders: Vec<Address> = self.pending_transactions.iter().map(|r| *r.key()).collect();
        
        while current_gas < target_gas_limit && !active_senders.is_empty() {
            let mut best_sender: Option<Address> = None;
            let mut best_tip: u128 = 0;
            let mut best_blobs: u32 = 0;

            let mut to_remove = Vec::new();
            for (idx, sender) in active_senders.iter().enumerate() {
                let current_idx = sender_indices.get(sender).unwrap_or(&0);
                let queue = match self.pending_transactions.get(sender) {
                    Some(q) => q,
                    None => {
                        to_remove.push(idx);
                        continue;
                    }
                };
                
                if let Some(tx) = queue.get(*current_idx) {
                    let tip = Self::calculate_effective_tip(tx, base_fee);
                    if tip == 0 && tx.max_fee_per_gas() < base_fee.to::<u128>() {
                        to_remove.push(idx);
                        continue;
                    }
                    
                    if let (Some(blob_fee), Transaction::Eip4844(s)) = (blob_base_fee, tx) {
                        if s.max_fee_per_blob_gas().unwrap_or_default() < blob_fee.to::<u128>() {
                            to_remove.push(idx);
                            continue;
                        }
                    }

                    let tx_blobs = if let Transaction::Eip4844(signed_tx) = tx {
                        signed_tx.tx().blob_versioned_hashes().map(|h| h.len()).unwrap_or(0) as u32
                    } else {
                        0
                    };

                    if current_gas + tx.gas_limit() <= target_gas_limit && current_blobs + tx_blobs <= max_blobs {
                        if tip > best_tip || (tip == best_tip && (best_sender.is_none() || tx_blobs > best_blobs)) {
                            best_tip = tip;
                            best_sender = Some(*sender);
                            best_blobs = tx_blobs;
                        }
                    } else {
                        to_remove.push(idx);
                    }
                } else {
                    to_remove.push(idx);
                }
            }

            to_remove.sort_unstable_by(|a, b| b.cmp(a));
            for idx in to_remove {
                active_senders.swap_remove(idx);
            }

            if let Some(sender) = best_sender {
                let current_idx = sender_indices.entry(sender).or_insert(0);
                let queue = self.pending_transactions.get(&sender).unwrap();
                let tx = &queue[*current_idx];
                
                let mut all_blobs_present = true;
                if let Transaction::Eip4844(signed_tx) = tx {
                    if let Some(hashes) = signed_tx.tx().blob_versioned_hashes() {
                        for hash in hashes {
                            if !self.blobs.contains_key(hash) {
                                all_blobs_present = false;
                                break;
                            }
                        }
                    }
                }

                if all_blobs_present {
                    current_gas += tx.gas_limit();
                    current_blobs += best_blobs;
                    result.push(tx.clone());
                    *current_idx += 1;
                } else {
                    active_senders.retain(|s| *s != sender);
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
        
        let queued_senders: Vec<Address> = self.queued_transactions.iter().map(|r| *r.key()).collect();
        for sender in queued_senders {
            let next_expected = self.pending_transactions.get(&sender)
                .and_then(|q| q.back().map(|t| t.nonce() + 1));
            
            if let Some(next_nonce) = next_expected {
                self.promote_queued(sender, next_nonce);
            }
        }

        let mut pending_copy: HashMap<Address, VecDeque<Transaction>> = self.pending_transactions.iter()
            .map(|r| (*r.key(), r.value().clone())).collect();
        
        while result.len() < n {
            let mut best_sender: Option<Address> = None;
            let mut best_gas_price: u128 = 0;

            for (address, queue) in &pending_copy {
                if let Some(tx) = queue.front() {
                    let gas_price = tx.gas_price().unwrap_or_default();
                    if gas_price > best_gas_price {
                        best_gas_price = gas_price;
                        best_sender = Some(*address);
                    }
                }
            }

            if let Some(sender) = best_sender {
                if let Some(queue) = pending_copy.get_mut(&sender) {
                    if let Some(tx) = queue.pop_front() {
                        result.push(tx);
                    }
                    if queue.is_empty() {
                        pending_copy.remove(&sender);
                    }
                }
            } else {
                break;
            }
        }
        
        result
    }

    /// Pop N transactions from the mempool for inclusion in a block.
    pub async fn pop_transactions(&self, n: usize) -> Vec<Transaction> {
        let mut result = Vec::with_capacity(n);
        let base_fee = *self.base_fee.read().await;
        
        while result.len() < n {
            let mut best_sender: Option<Address> = None;
            let mut best_tip: u128 = 0;

            for r in self.pending_transactions.iter() {
                if let Some(tx) = r.value().front() {
                    let tip = Self::calculate_effective_tip(tx, base_fee);
                    if tip > best_tip || (tip == best_tip && best_sender.is_none()) {
                        best_tip = tip;
                        best_sender = Some(*r.key());
                    }
                }
            }

            if let Some(sender) = best_sender {
                let mut queue = self.pending_transactions.get_mut(&sender).unwrap();
                if let Some(tx) = queue.pop_front() {
                    result.push(tx);
                }
                if queue.is_empty() {
                    drop(queue);
                    self.pending_transactions.remove(&sender);
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
    pub async fn enforce_capacity(&self, max_size: usize) {
        if self.len() <= max_size {
            return;
        }
        
        let base_fee = *self.base_fee.read().await;
        while self.len() > max_size {
            let mut worst_sender: Option<(Address, bool)> = None; // (address, is_pending)
            let mut worst_tip: u128 = u128::MAX;

            // Prioritize evicting from queued first
            for r in self.queued_transactions.iter() {
                if let Some(tx) = r.value().back() {
                    let tip = Self::calculate_effective_tip(tx, base_fee);
                    if tip < worst_tip {
                        worst_tip = tip;
                        worst_sender = Some((*r.key(), false));
                    }
                }
            }

            // If still over capacity and queued is empty or we have even worse pending
            if worst_sender.is_none() || self.queued_transactions.is_empty() {
                for r in self.pending_transactions.iter() {
                    if let Some(tx) = r.value().back() {
                        let tip = Self::calculate_effective_tip(tx, base_fee);
                        if tip < worst_tip {
                            worst_tip = tip;
                            worst_sender = Some((*r.key(), true));
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

                if let Some(mut q) = queue {
                    if let Some(tx) = q.pop_back() {
                        info!("[Mempool] Evicting transaction {:?} (nonce: {}) from {} pool due to capacity (tip: {})", 
                            tx.hash(), tx.nonce(), if is_pending { "pending" } else { "queued" }, worst_tip);
                    }
                    if q.is_empty() {
                        drop(q);
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
        self.pending_transactions.iter().map(|r| r.value().len()).sum::<usize>() +
        self.queued_transactions.iter().map(|r| r.value().len()).sum::<usize>()
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
            Signature::new(U256::from(1), U256::from(1), true),
            B256::repeat_byte(1),
        ));

        mempool.inner.add_transaction(tx1.clone(), 0).await;
        mempool.inner.add_transaction(tx2.clone(), 0).await;

        // Peek with gas limit enough for both
        let best = mempool.peek_best_transactions(50000, U256::from(100), None, None).await;
        assert_eq!(best.len(), 2);
        // Note: Legacy transactions with same gas price might have non-deterministic order if sender hashes are similar,
        // but here they have different gas prices.
        // tx1 gas_price=150, base_fee=100 -> tip=50
        // tx2 gas_price=200, base_fee=100 -> tip=100
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
            Signature::new(U256::from(1), U256::from(1), true),
            B256::repeat_byte(1),
        ));

        let inner = &mempool.inner;
        let from = tx1.recover_signer().unwrap_or_default();
        inner.add_transaction(tx1.clone(), 0).await;
        inner.pending_transactions.entry(from).or_default().push_back(tx2.clone());

        let best = mempool.peek_best_transactions(100000, U256::from(100), None, None).await;
        assert_eq!(best.len(), 2);
        assert_eq!(best[0].hash(), tx1.hash()); // Nonce 0 must come first even if Tx2 has higher tip
        assert_eq!(best[1].hash(), tx2.hash());
    }

    #[tokio::test]
    async fn test_mempool_new() {
        let mempool = Mempool::new(U256::from(100));
        assert_eq!(*mempool.inner.base_fee.read().await, U256::from(100));
        assert!(mempool.inner.len() == 0);
    }

    #[tokio::test]
    async fn test_add_transaction_nonce_low() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 5,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        
        // current_nonce = 10, tx_nonce = 5 -> should be rejected (false)
        assert!(!mempool.inner.add_transaction(tx, 10).await);
        assert!(mempool.inner.is_empty());
    }

    #[tokio::test]
    async fn test_add_transaction_pending() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 10,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        
        // current_nonce = 10, tx_nonce = 10 -> should be pending
        assert!(mempool.inner.add_transaction(tx, 10).await);
        assert_eq!(mempool.inner.len(), 1);
        assert_eq!(mempool.inner.pending_transactions.len(), 1);
    }

    #[tokio::test]
    async fn test_add_transaction_queued() {
        let mempool = Mempool::new(U256::from(0));
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 15,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        
        // current_nonce = 10, tx_nonce = 15 -> should be queued
        assert!(mempool.inner.add_transaction(tx, 10).await);
        assert_eq!(mempool.inner.len(), 1);
        assert_eq!(mempool.inner.queued_transactions.len(), 1);
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
            let inner = &mempool.inner;
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
        
        mempool.inner.add_transaction(tx, 10).await;
        assert_eq!(mempool.inner.len(), 1);
        
        mempool.inner.remove_transactions(&[hash]);
        assert_eq!(mempool.inner.len(), 0);
    }

    #[tokio::test]
    async fn test_transaction_replacement() {
        let mempool = Mempool::new(U256::from(0));
        let signature = Signature::new(U256::from(1), U256::from(1), true);
        
        let tx1 = Transaction::Legacy(TxLegacy {
            nonce: 10,
            gas_price: 100,
            ..Default::default()
        }.into_signed(signature.clone()));
        
        // Add first transaction
        assert!(mempool.inner.add_transaction(tx1.clone(), 10).await);
        assert_eq!(mempool.inner.len(), 1);
        
        // Try to replace it with same nonce but higher fee (10% bump)
        let tx2 = Transaction::Legacy(TxLegacy {
            nonce: 10,
            gas_price: 110, // 100 + 10% = 110
            ..Default::default()
        }.into_signed(signature));
        
        // Use nonce_lookup to simulate Engine API behavior
        let addr1 = tx1.recover_signer().unwrap();
        let (state_nonce_lookup, next_pending) = mempool.nonce_lookup(addr1, 10).await;
        assert_eq!(state_nonce_lookup, 10);
        assert_eq!(next_pending, 11);
        
        // Engine API now passes state_nonce (10) to add_transaction
        assert!(mempool.inner.add_transaction(tx2, state_nonce_lookup).await);
        
        // It should have REPLACED tx1, so len is still 1
        assert_eq!(mempool.inner.len(), 1, "Mempool length should be 1 after replacement");
        
        // Try to replace with lower fee (should fail)
        let tx3 = Transaction::Legacy(TxLegacy {
            nonce: 10,
            gas_price: 105,
            ..Default::default()
        }.into_signed(Signature::test_signature()));
        
        assert!(!mempool.inner.add_transaction(tx3, 10).await);
        assert_eq!(mempool.inner.len(), 1);
    }
}
