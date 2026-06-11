
use std::sync::Arc;
use wasix_eth_core::mempool::mempool::Mempool;
use wasix_eth_storage::EthDatabase;
use wasix_eth_types::{
    Block, BlockBody, Header, Transaction, Address, B256, U256, Bytes, 
    PayloadStatusEnum, ExecutionPayloadV1, BlockId, Hardfork, ChainConfig,
    Receipt, Bloom, B64, EMPTY_OMMER_ROOT_HASH
};
use wasix_eth_core::chain_manager::NoopChainManager;
use tempfile::tempdir;
use tokio::sync::broadcast;
use wasix_eth_core::account_manager::AccountManager;
use wasix_eth_core::{Engine, EthConsensus};
use wasix_eth_execution::execution_provider::{ExecutionProvider, TransactionExecutionResult};
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{HeaderWriter, BlockWriter};
use wasix_eth_utils::info;

struct MockExecutionProvider;
impl ExecutionProvider for MockExecutionProvider {
    fn execute_block_for_payload(&self, _txs: Vec<Transaction>, _parent: &Header, _attr: &wasix_eth_types::PayloadAttributes, _base_fee: Option<u64>) -> wasix_eth_types::Result<(Block<Transaction>, Vec<Receipt>)> {
        Ok((Block::default(), vec![]))
    }
    fn execute_block(&self, block: Block<Transaction>) -> wasix_eth_types::Result<(Block<Transaction>, Vec<Receipt>)> {
        Ok((block, vec![]))
    }
    fn execute_block_with_commit(&self, block: Block<Transaction>, _commit: bool) -> anyhow::Result<(Block<Transaction>, Vec<Receipt>)> {
        Ok((block, vec![]))
    }
    fn execute_block_with_state_root(&self, block: Block<Transaction>, _commit: bool, _state_root: Option<B256>) -> anyhow::Result<(Block<Transaction>, Vec<Receipt>)> {
        Ok((block, vec![]))
    }
    fn run_execution(&self, _txs: Vec<Transaction>, _block: Block<Transaction>, _apply: bool) -> wasix_eth_types::Result<(Vec<TransactionExecutionResult>, Block<Transaction>)> {
        Ok((vec![], _block))
    }
    fn run_execution_with_state_root(&self, _txs: Vec<Transaction>, _block: Block<Transaction>, _apply: bool, _state_root: Option<B256>) -> wasix_eth_types::Result<(Vec<TransactionExecutionResult>, Block<Transaction>)> {
        Ok((vec![], _block))
    }
    fn execute_block_with_batch(&self, block: Block<Transaction>, _batch: &wasix_eth_storage::write::BatchWriter, _state_root: Option<B256>) -> anyhow::Result<(Block<Transaction>, Vec<Receipt>)> {
        Ok((block, vec![]))
    }
}

async fn setup_engine() -> (Engine, Arc<EthDatabase>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let db = EthDatabase::open(&db_path).unwrap();
    db.init_tables().unwrap();
    let db_arc = Arc::new(db);
    let read_storage = DatabaseReadProvider::new(db_arc.inner());
    let write_storage = DatabaseWriteProvider::new(db_arc.inner());
    let execution = Arc::new(MockExecutionProvider);
    let account_manager = Arc::new(AccountManager::new_with_dev_keys());
    let (event_tx, _) = broadcast::channel(10);
    let mempool = Arc::new(Mempool::new(U256::ZERO));
    let chain = Arc::new(NoopChainManager);

    let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
    (Engine::new(read_storage, write_storage, execution, account_manager, chain, mempool, event_tx, consensus), db_arc, dir)
}

#[tokio::test]
async fn test_new_payload_bad_hash_returns_null_valid_hash() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());

    // 1. Insert genesis block
    let genesis_header = Header {
        number: 0,
        parent_hash: B256::ZERO,
        beneficiary: Address::ZERO,
        state_root: B256::repeat_byte(0x01),
        transactions_root: B256::repeat_byte(0x02),
        receipts_root: B256::repeat_byte(0x03),
        logs_bloom: Bloom::default(),
        difficulty: U256::from(0x30000),
        gas_limit: 30_000_000,
        gas_used: 0,
        timestamp: 1234,
        extra_data: Bytes::new(),
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(1_000_000_000),
        withdrawals_root: None,
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        ommers_hash: B256::ZERO,
        requests_hash: None,
    };
    let genesis_hash = genesis_header.hash_slow();
    writer.insert_header(0, genesis_header.clone()).unwrap();
    writer.insert_block_hash(genesis_hash, 0).unwrap();
    writer.set_canonical(0, genesis_hash).unwrap();

    // 2. Prepare bad payload
    let mut payload = ExecutionPayloadV1 {
        parent_hash: genesis_hash,
        fee_recipient: Address::ZERO,
        state_root: B256::repeat_byte(0x01),
        receipts_root: B256::repeat_byte(0x03),
        logs_bloom: Bloom::default(),
        prev_randao: B256::ZERO,
        block_number: 1,
        gas_limit: 30_000_000,
        gas_used: 0,
        timestamp: 1235,
        extra_data: Bytes::new(),
        base_fee_per_gas: U256::from(1_000_000_000),
        block_hash: B256::repeat_byte(0x99), // INTENTIONALLY BAD HASH
        transactions: vec![],
    };

    // 3. Call new_payload
    let result = engine.new_payload(payload.clone(), None).await.unwrap();

    // 4. Assertions
    if let PayloadStatusEnum::Invalid { validation_error } = result.status {
        assert_eq!(validation_error, "Block hash mismatch");
    } else {
        panic!("Expected INVALID status, got {:?}", result.status);
    }
    
    // This is what Hive expects: null (None)
    assert_eq!(result.latest_valid_hash, None, "latest_valid_hash must be null on block hash mismatch");
}
