use std::collections::{HashMap, VecDeque};
use alloy_primitives::{Address, B256, U256};
use crate::storage::Transaction;

/// A simple mempool to store pending transactions.
#[derive(Debug, Default, Clone)]
pub struct Mempool {
    /// Pending transactions organized by sender and nonce.
    pub pending_transactions: HashMap<Address, VecDeque<Transaction>>,
    /// Current base fee for transactions in the mempool.
    pub base_fee: U256,
}

impl Mempool {
    /// Create a new mempool with the given base fee.
    pub fn new(base_fee: U256) -> Self {
        Self {
            pending_transactions: HashMap::new(),
            base_fee,
        }
    }

    /// Add a transaction to the mempool.
    pub fn add_transaction(&mut self, tx: Transaction) {
        let queue = self.pending_transactions
            .entry(tx.from)
            .or_insert_with(VecDeque::new);
            
        // Insert in nonce order
        let pos = queue.binary_search_by_key(&tx.nonce, |t| t.nonce)
            .unwrap_or_else(|e| e);
        
        // If a transaction with the same nonce exists, we might want to replace it
        // if the new one has a higher gas price. For now, let's just insert it.
        if pos < queue.len() && queue[pos].nonce == tx.nonce {
            if tx.gas_price > queue[pos].gas_price {
                queue[pos] = tx;
            }
        } else {
            queue.insert(pos, tx);
        }
    }

    /// Get all transactions currently in the mempool.
    pub fn get_all_transactions(&self) -> Vec<Transaction> {
        self.pending_transactions
            .values()
            .flatten()
            .cloned()
            .collect()
    }

    /// Clear the mempool.
    pub fn clear(&mut self) {
        self.pending_transactions.clear();
    }

    /// Remove transactions that have been included in a block.
    pub fn remove_transactions(&mut self, tx_hashes: &[B256]) {
        for queue in self.pending_transactions.values_mut() {
            queue.retain(|tx| !tx_hashes.contains(&tx.hash));
        }
        // Clean up empty queues
        self.pending_transactions.retain(|_, queue| !queue.is_empty());
    }

    /// Pop N transactions from the mempool for inclusion in a block.
    /// Transactions are prioritized by gas price while strictly respecting nonce order for each sender.
    pub fn pop_transactions(&mut self, n: usize) -> Vec<Transaction> {
        let mut result = Vec::with_capacity(n);
        
        // We will keep track of which transaction is "next" for each sender.
        // For each sender, the "next" transaction is at the front of their VecDeque.
        
        while result.len() < n {
            let mut best_sender: Option<Address> = None;
            let mut best_gas_price: U256 = U256::ZERO;

            for (address, queue) in &self.pending_transactions {
                if let Some(tx) = queue.front() {
                    // Check if it's better than current best
                    if tx.gas_price > best_gas_price {
                        best_gas_price = tx.gas_price;
                        best_sender = Some(*address);
                    }
                }
            }

            if let Some(sender) = best_sender {
                // Remove from queue and add to result
                if let Some(queue) = self.pending_transactions.get_mut(&sender) {
                    if let Some(tx) = queue.pop_front() {
                        result.push(tx);
                    }
                    if queue.is_empty() {
                        self.pending_transactions.remove(&sender);
                    }
                }
            } else {
                // No more transactions in mempool
                break;
            }
        }
        
        result
    }

    /// Get the total count of transactions in the mempool.
    pub fn len(&self) -> usize {
        self.pending_transactions.values().map(|q| q.len()).sum()
    }

    /// Check if the mempool is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
