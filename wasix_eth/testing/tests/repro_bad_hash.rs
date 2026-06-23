
use std::sync::Arc;
use wasix_eth_core::mempool::mempool::Mempool;
use wasix_eth_storage::EthDatabase;
use wasix_eth_types::{
    Block, Header, Transaction, Address, B256, U256, Bytes, 
    PayloadStatusEnum, ExecutionPayloadV1, Hardfork, ChainConfig,
    Receipt, Bloom, B64
};
use wasix_eth_core::chain_manager::NoopChainManager;
use tempfile::tempdir;
use tokio::sync::broadcast;
use wasix_eth_core::account_manager::AccountManager;
use wasix_eth_core::{Engine, EthConsensus};
use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider, TransactionExecutionResult};
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{HeaderWriter, BlockWriter};


async fn setup_engine() -> (Engine, Arc<EthDatabase>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let db = EthDatabase::open(&db_path).unwrap();
    db.init_tables().unwrap();
    let db_arc = Arc::new(db);
    let read_storage = DatabaseReadProvider::new(db_arc.inner());
    let write_storage = DatabaseWriteProvider::new(db_arc.inner());
    let execution = Arc::new(EthExecutionProvider::new(read_storage.clone(), write_storage.clone()));
    let account_manager = Arc::new(AccountManager::new_with_dev_keys());
    let (event_tx, _) = broadcast::channel(10);
    let mempool = Arc::new(Mempool::new(U256::ZERO));
    let chain = Arc::new(NoopChainManager);

    let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
    let canonical = Arc::new(wasix_eth_core::engine::canonicality_tracker::CanonicalState::new(read_storage.clone(), write_storage.clone()));
    let block_tree = Arc::new(wasix_eth_core::engine::sidechain_tracker::BlockTree::new());
    let reorg_handler = Arc::new(wasix_eth_core::engine::reorg_manager::ReorgHandler::new(read_storage.clone(), write_storage.clone(), canonical.clone()));
    let rpc_engine = Arc::new(wasix_eth_core::engine::api::RPCEngine::new(
        read_storage.clone(),
        write_storage.clone(),
        account_manager.clone(),
        chain.clone(),
        mempool.clone(),
        event_tx.clone(),
        consensus.clone(),
        execution.clone(),
    ));

    (Engine::new(
        read_storage.clone(),
        write_storage.clone(),
        execution,
        account_manager,
        mempool.clone(),
        event_tx,
        consensus,
        rpc_engine,
        chain.clone(),
        canonical,
        block_tree,
        reorg_handler,
        wasix_eth_core::engine::forkchoice_validator::ForkchoiceValidator::new(read_storage.clone(), chain.clone()),
        Arc::new(wasix_eth_core::mempool::listener::MempoolListener::new(mempool, read_storage)),
    ), db_arc, dir)
}

#[tokio::test]
async fn test_new_payload_bad_hash_repro() {
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

    // 2. Prepare payload with mismatched hash
    let payload = ExecutionPayloadV1 {
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
        block_hash: B256::repeat_byte(0xEE), // INVALID HASH
        transactions: vec![],
    };

    // 3. Call new_payload
    let result = engine.new_payload(payload.clone(), None).await.unwrap();

    // 4. Verify results
    assert!(matches!(result.status, PayloadStatusEnum::Invalid { .. }));
    if let PayloadStatusEnum::Invalid { validation_error } = &result.status {
        assert_eq!(validation_error, "Block hash mismatch");
    }
    
    // Hive expects null (None) when block hash is invalid
    assert_eq!(result.latest_valid_hash, None, "latestValidHash must be null on block hash mismatch");
}
