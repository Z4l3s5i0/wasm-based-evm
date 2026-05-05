use std::sync::Arc;
use tokio::sync::RwLock;
use crate::storage::traits::{StateProvider, SyncStateProvider, StateSnapshot};
use crate::storage::storage::InMemoryStorage;
use alloy_consensus::{Block, Header, transaction::SignerRecoverable};
use alloy_primitives::{Bytes, B256, U256};
use crate::{info, debug, error};
use crate::evm::executor::Executor;
use crate::misc::metrics::{BLOCK_PRODUCTION_SUCCESS, BLOCK_PRODUCTION_FAILED};
use crate::storage::mempool::Mempool;

pub struct DevMode {
    mempool: Arc<RwLock<Mempool>>,
    state_storage: Arc<dyn StateProvider>,
    executor: Arc<Executor>,
    interval: u64,
}

impl DevMode {
    pub fn new(
        mempool: Arc<RwLock<Mempool>>,
        state_storage: Arc<dyn StateProvider>,
        executor: Arc<Executor>,
        interval: u64,
    ) -> Self {
        Self {
            mempool,
            state_storage,
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
        let mut storage_snapshot = self.state_storage.get_snapshot().await
            .map_err(|e| e.to_string())?;
        let mut mempool = self.mempool.write().await;

        let storage = storage_snapshot.as_any().downcast_ref::<InMemoryStorage>()
            .ok_or_else(|| "Failed to downcast storage".to_string())?;

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
        let _tx_hashes: Vec<B256> = transactions.iter().map(|tx| tx.hash()).copied().collect();

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

        debug!("[DevMode] Producing block #{} with {} transactions", number, transactions.len());

        match self.executor.execute_with_changeset(storage_snapshot.as_any_mut().downcast_mut::<InMemoryStorage>().unwrap(), transactions.clone(), block.clone()) {
            Ok((_, receipts, changeset)) => {
                BLOCK_PRODUCTION_SUCCESS.inc();
                let block_hash = block.header.hash_slow();
                let new_base_fee = U256::from(block.header.base_fee_per_gas.unwrap_or_default());
                
                let writer = self.state_storage.writer();
                let withdrawals = block.body.withdrawals.clone().unwrap_or_default();
                writer.commit_block(block, receipts, changeset, withdrawals.into_iter().collect()).await.map_err(|e| e.to_string())?;
                writer.update_forkchoice(block_hash, Some(block_hash), Some(block_hash)).await.map_err(|e| e.to_string())?;
                
                debug!("[DevMode] Block #{} produced successfully: {:?}", number, block_hash);

                // Update mempool base fee (this will trigger revalidation and eviction if needed)
                mempool.update_base_fee(new_base_fee, storage_snapshot.as_any().downcast_ref::<InMemoryStorage>().unwrap()).await;

                Ok(())
            }
            Err(e) => {
                BLOCK_PRODUCTION_FAILED.inc();
                // If block execution fails, we should put transactions back into mempool
                info!("[DevMode] Block execution failed: {}. Putting {} transactions back into mempool.", e, transactions.len());
                for tx in transactions {
                    let from = tx.recover_signer().unwrap_or_default();
                    let current_nonce = self.state_storage.transaction_count(from, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or(0);
                    mempool.add_transaction(tx, current_nonce);
                }
                Err(format!("Block execution failed: {}", e))
            }
        }
    }
}
