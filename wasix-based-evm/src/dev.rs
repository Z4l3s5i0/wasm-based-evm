use std::sync::Arc;
use tokio::sync::RwLock;
use crate::mempool::Mempool;
use crate::storage::storage::InMemoryStorage;
use crate::storage::traits::StateProvider;
use crate::executor::Executor;
use alloy_consensus::{Block, Header, transaction::SignerRecoverable};
use alloy_primitives::{Bytes, B256};
use crate::{info, error};

pub struct DevMode {
    mempool: Arc<RwLock<Mempool>>,
    storage: Arc<RwLock<InMemoryStorage>>,
    executor: Arc<Executor>,
    interval: u64,
}

impl DevMode {
    pub fn new(
        mempool: Arc<RwLock<Mempool>>,
        storage: Arc<RwLock<InMemoryStorage>>,
        executor: Arc<Executor>,
        interval: u64,
    ) -> Self {
        Self {
            mempool,
            storage,
            executor,
            interval,
        }
    }

    pub async fn start(self) {
        info!("[DevMode] Starting automatic block production every {} seconds", self.interval);
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(self.interval));
        
        // Skip the first tick which happens immediately
        interval.tick().await;

        loop {
            interval.tick().await;
            if let Err(e) = self.produce_block().await {
                error!("[DevMode] Failed to produce block: {}", e);
            }
        }
    }

    async fn produce_block(&self) -> Result<(), String> {
        let mut storage = self.storage.write().await;
        let mut mempool = self.mempool.write().await;

        let latest_block = storage.get_latest_block()
            .cloned()
            .ok_or_else(|| "Latest block not found".to_string())?;

        let parent_hash = latest_block.header.hash_slow();
        let number = latest_block.header.number + 1;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Ensure timestamp is always increasing
        let timestamp = if timestamp <= latest_block.header.timestamp {
            latest_block.header.timestamp + 1
        } else {
            timestamp
        };

        // Take transactions from mempool
        let transactions = mempool.pop_transactions(10);
        let tx_hashes: Vec<B256> = transactions.iter().map(|tx| tx.hash()).copied().collect();

        let header = Header {
            parent_hash,
            number,
            timestamp,
            beneficiary: latest_block.header.beneficiary,
            gas_limit: latest_block.header.gas_limit,
            base_fee_per_gas: latest_block.header.base_fee_per_gas,
            extra_data: Bytes::new(),
            mix_hash: B256::ZERO,
            state_root: B256::ZERO,
            transactions_root: B256::ZERO,
            receipts_root: B256::ZERO,
            ..Default::default()
        };

        let block = Block {
            header,
            body: alloy_consensus::BlockBody {
                transactions: transactions.clone(),
                ommers: vec![],
                withdrawals: None,
            },
        };

        info!("[DevMode] Producing block #{} with {} transactions", number, transactions.len());

        match self.executor.execute_block(&mut storage, transactions.clone(), block.clone()) {
            Ok(_) => {
                let block_hash = block.header.hash_slow();
                storage.add_block(block);
                storage.update_forkchoice(block_hash);
                
                info!("[DevMode] Block #{} produced successfully: {:?}", number, block_hash);

                // Revalidate mempool after successful block production
                mempool.revalidate(&*storage).await;

                Ok(())
            }
            Err(e) => {
                // If block execution fails, we should put transactions back into mempool
                info!("[DevMode] Block execution failed: {}. Putting {} transactions back into mempool.", e, transactions.len());
                for tx in transactions {
                    let from = tx.recover_signer().unwrap_or_default();
                    let current_nonce = storage.transaction_count(from, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or(0);
                    mempool.add_transaction(tx, current_nonce);
                }
                Err(format!("Block execution failed: {}", e))
            }
        }
    }
}
