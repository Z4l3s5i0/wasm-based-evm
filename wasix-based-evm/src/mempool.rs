use std::collections::{HashMap, VecDeque};
use alloy_primitives::{Address, B256, U256};
use alloy_consensus::{TxEnvelope as Transaction, Transaction as _, transaction::SignerRecoverable as _};
use crate::storage::traits::StateProvider;
use crate::info;

#[derive(Debug, Default, Clone)]
pub struct Mempool {
    /// Transactions ready for inclusion (no nonce gaps, sufficient balance).
    pub pending_transactions: HashMap<Address, VecDeque<Transaction>>,
    /// Transactions waiting for a previous nonce to arrive.
    pub queued_transactions: HashMap<Address, VecDeque<Transaction>>,
    /// Current base fee for transactions in the mempool.
    pub base_fee: U256,
}

/// Metadata for a transaction in the mempool.
#[derive(Debug, Clone)]
pub struct TxMetadata {
    pub arrival_time: u64,
    pub effective_tip: u128,
}

impl Mempool {
    /// Create a new mempool with the given base fee.
    pub fn new(base_fee: U256) -> Self {
        Self {
            pending_transactions: HashMap::new(),
            queued_transactions: HashMap::new(),
            base_fee,
        }
    }

    /// Add a transaction to the mempool, considering its nonce and the current state nonce.
    /// Returns true if the transaction was newly added or replaced an existing one.
    pub fn add_transaction(&mut self, tx: Transaction, current_nonce: u64) -> bool {
        let hash = tx.hash();
        let from = tx.recover_signer().unwrap_or_default();
        let tx_nonce = tx.nonce();

        if tx_nonce < current_nonce {
            info!("[Mempool] Rejecting transaction {:?} (nonce: {}) because it is below current nonce ({})", hash, tx_nonce, current_nonce);
            return false;
        }

        // Potential addition: check balance against current state
        // This would require passing storage or balance to this method.

        if tx_nonce == current_nonce {
            // Can be pending
            self.insert_into_queue(from, tx, true)
        } else {
            // Potential gap, goes to queued
            self.insert_into_queue(from, tx, false)
        }
    }

    /// Set the current base fee and re-evaluate pending pool.
    pub fn update_base_fee(&mut self, new_base_fee: U256) {
        self.base_fee = new_base_fee;
        // In a real client, we might evict transactions that can no longer pay the base fee.
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
    }

    /// Peek at N transactions from the mempool for block building without removing them.
    pub fn peek_transactions(&self, n: usize) -> Vec<Transaction> {
        let mut result = Vec::with_capacity(n);
        let mut copy = self.clone();
        
        while result.len() < n {
            let mut best_sender: Option<Address> = None;
            let mut best_gas_price: U256 = U256::ZERO;

            for (address, queue) in &copy.pending_transactions {
                if let Some(tx) = queue.front() {
                    let gas_price = U256::from(tx.gas_price().unwrap_or_default());
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
                    let tip = self.calculate_effective_tip(tx);
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
        
        result
    }

    /// Calculate the effective tip for a transaction based on the current base fee.
    fn calculate_effective_tip(&self, tx: &Transaction) -> u128 {
        let base_fee = self.base_fee.to::<u128>();
        match tx {
            Transaction::Legacy(signed) => {
                let gas_price = signed.tx().gas_price;
                if gas_price < base_fee {
                    0
                } else {
                    gas_price - base_fee
                }
            }
            Transaction::Eip2930(signed) => {
                let gas_price = signed.tx().gas_price;
                if gas_price < base_fee {
                    0
                } else {
                    gas_price - base_fee
                }
            }
            Transaction::Eip1559(signed) => {
                let max_fee = signed.tx().max_fee_per_gas;
                let max_priority_fee = signed.tx().max_priority_fee_per_gas;
                
                if max_fee < base_fee {
                    0
                } else {
                    let max_tip = max_fee - base_fee;
                    max_tip.min(max_priority_fee)
                }
            }
            Transaction::Eip4844(signed) => {
                let max_fee = signed.tx().max_fee_per_gas();
                let max_priority_fee = signed.tx().max_priority_fee_per_gas();
                
                if max_fee < base_fee {
                    0
                } else {
                    let max_tip = max_fee - base_fee;
                    max_tip.min(max_priority_fee.unwrap_or_default())
                }
            }
            _ => 0,
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
                    let tip = self.calculate_effective_tip(tx);
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
                        let tip = self.calculate_effective_tip(tx);
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

    /// Revalidate the mempool against the latest state.
    /// Removes transactions that are no longer valid (e.g. nonce too low, insufficient balance).
    pub async fn revalidate(&mut self, state: &dyn StateProvider) {
        let mut to_remove = Vec::new();

        // Check pending transactions
        for (address, queue) in &self.pending_transactions {
            let current_nonce = state.transaction_count(*address, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or(0);
            let current_balance = state.balance(*address, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or_default();

            for tx in queue {
                let tx_nonce = tx.nonce();
                // A very simplified balance check: gas_limit * gas_price + value
                let gas_limit = tx.gas_limit() as u128;
                let gas_price = tx.gas_price().unwrap_or_default();
                let value = tx.value().to::<u128>();
                let required_balance = U256::from(gas_limit * gas_price + value);

                if tx_nonce < current_nonce || current_balance < required_balance {
                    to_remove.push(tx.hash().clone());
                }
            }
        }

        // Check queued transactions
        for (address, queue) in &self.queued_transactions {
            let current_nonce = state.transaction_count(*address, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or(0);
            let current_balance = state.balance(*address, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or_default();

            for tx in queue {
                let tx_nonce = tx.nonce();
                let gas_limit = tx.gas_limit() as u128;
                let gas_price = tx.gas_price().unwrap_or_default();
                let value = tx.value().to::<u128>();
                let required_balance = U256::from(gas_limit * gas_price + value);

                if tx_nonce < current_nonce || current_balance < required_balance {
                    to_remove.push(tx.hash().clone());
                }
            }
        }

        if !to_remove.is_empty() {
            info!("[Mempool] Removing {} invalid transactions during revalidation", to_remove.len());
            self.remove_transactions(&to_remove);
        }

        // Try to promote queued transactions in case some were removed or new ones can be moved
        let addresses: Vec<Address> = self.queued_transactions.keys().copied().collect();
        for address in addresses {
            let current_nonce = state.transaction_count(address, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or(0);
            self.promote_queued(address, current_nonce);
        }
    }
}
