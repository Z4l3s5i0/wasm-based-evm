#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;
    use wasix_eth_core::{ChainManagerImpl, Engine, EthConsensus};
    use wasix_eth_core::account_manager::AccountManager;
    use wasix_eth_core::chain_manager::ChainManager;
    use wasix_eth_types::SyncStatus;
    use wasix_eth_types::async_trait;
    use wasix_eth_storage::EthDatabase;
    use wasix_eth_storage::read::DatabaseReadProvider;
    use wasix_eth_storage::write::DatabaseWriteProvider;
    use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider, TransactionExecutionResult};
    use wasix_eth_types::{Block, Transaction, Header, Receipt, PayloadAttributes, B256};
    use wasix_eth_core::mempool::mempool::Mempool;
    use tokio::sync::broadcast::channel;
    use wasix_eth_storage::read_traits::StorageProvider;
    use wasix_eth_sync::processor::BlockProcessor;

    fn setup_engine() -> (Arc<Engine>, Arc<EthDatabase>, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test_db.redb");
        let db = EthDatabase::open(&db_path).unwrap();
        let db = Arc::new(db);

        let read_storage = DatabaseReadProvider::new(db.inner());
        let write_storage = DatabaseWriteProvider::new(db.inner());
        let execution = Arc::new(EthExecutionProvider::new(read_storage.clone(), write_storage.clone()));
        let account_manager = Arc::new(AccountManager::new());
        let chain = Arc::new(ChainManagerImpl::new(read_storage.clone(), write_storage.clone()));
        let mempool = Arc::new(Mempool::new(wasix_eth_types::U256::ZERO));
        let (event_tx, _event_rx) = channel(100);

        let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
        let engine = Engine::new(
            read_storage,
            write_storage,
            execution,
            account_manager,
            chain,
            mempool,
            event_tx,
            consensus,
        );

        (Arc::new(engine), db, temp_dir)
    }

    #[tokio::test]
    async fn test_process_block_success() {
        let (engine, _db, _temp) = setup_engine();
        let processor = BlockProcessor::new(engine);

        let block = Block {
            header: Header {
                number: 1,
                parent_hash: B256::ZERO, // Matches default header hash if not careful, but okay for test
                ..Default::default()
            },
            body: wasix_eth_types::BlockBody {
                transactions: vec![],
                ommers: vec![],
                withdrawals: None,
            },
        };


        let result = processor.process_block(block).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Syncing"));
    }

    #[tokio::test]
    async fn test_process_block_import_failure() {
        struct FailingExecutionProvider;
        impl ExecutionProvider for FailingExecutionProvider {
            fn execute_block_for_payload(&self, _txs: Vec<Transaction>, _parent: &Header, _attr: &PayloadAttributes, _base_fee: Option<u64>) -> wasix_eth_types::Result<(Block<Transaction>, Vec<Receipt>)> {
                unimplemented!()
            }
            fn execute_block(&self, _block: Block<Transaction>) -> wasix_eth_types::Result<(Block<Transaction>, Vec<Receipt>)> {
                Err(anyhow::anyhow!("Execution failed").into())
            }
            fn run_execution(&self, _txs: Vec<Transaction>, _block: Block<Transaction>, _apply: bool) -> wasix_eth_types::Result<(Vec<wasix_eth_execution::executor::TransactionExecutionResult>, Block<Transaction>)> {
                unimplemented!()
            }

            fn execute_block_with_commit(&self, block: Block<Transaction>, commit: bool) -> anyhow::Result<(Block<Transaction>, Vec<Receipt>)> {
                unimplemented!()
            }

            fn execute_block_with_state_root(&self, block: Block<Transaction>, commit: bool, state_root: Option<B256>) -> anyhow::Result<(Block<Transaction>, Vec<Receipt>)> {
                unimplemented!()
            }

            fn run_execution_with_state_root(&self, transactions: Vec<Transaction>, block: Block<Transaction>, apply_changes: bool, state_root: Option<B256>) -> anyhow::Result<(Vec<TransactionExecutionResult>, Block<Transaction>)> {
                unimplemented!()
            }
        }

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test_db_fail.redb");
        let db = EthDatabase::open(&db_path).unwrap();
        let db = Arc::new(db);

        let read_storage = DatabaseReadProvider::new(db.inner());
        let write_storage = DatabaseWriteProvider::new(db.inner());
        let execution = Arc::new(FailingExecutionProvider);
        let account_manager = Arc::new(AccountManager::new());
        let chain = Arc::new(ChainManagerImpl::new(read_storage.clone(), write_storage.clone()));
        let mempool = Arc::new(Mempool::new(wasix_eth_types::U256::ZERO));
        let (event_tx, _event_rx) = channel(100);

        let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
        let engine = Engine::new(
            read_storage,
            write_storage,
            execution,
            account_manager,
            chain,
            mempool,
            event_tx,
            consensus,
        );
        let engine = Arc::new(engine);
        let processor = BlockProcessor::new(engine);

        let block = Block {
            header: Header {
                number: 1,
                ..Default::default()
            },
            body: wasix_eth_types::BlockBody {
                transactions: vec![],
                ommers: vec![],
                withdrawals: None,
            },
        };

        let result = processor.process_block(block).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Import failed for block 1"));
    }
}