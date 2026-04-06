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
        self.pending_transactions
            .entry(tx.from)
            .or_insert_with(VecDeque::new)
            .push_back(tx);
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
    /// This is a simple implementation that takes transactions in the order they were added.
    pub fn pop_transactions(&mut self, n: usize) -> Vec<Transaction> {
        let mut result = Vec::with_capacity(n);
        let mut count = 0;
        
        // Collect all transactions and sort them somehow, or just take them as they are
        // For now, let's just flatten and take N.
        // A better implementation would prioritize by gas price and respect nonces.
        
        // Since we need to modify the map, we'll collect hashes to remove.
        let mut tx_hashes_to_remove = Vec::new();
        
        'outer: for queue in self.pending_transactions.values() {
            for tx in queue {
                if count < n {
                    result.push(tx.clone());
                    tx_hashes_to_remove.push(tx.hash);
                    count += 1;
                } else {
                    break 'outer;
                }
            }
        }
        
        self.remove_transactions(&tx_hashes_to_remove);
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
