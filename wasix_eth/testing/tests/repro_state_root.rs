
use std::sync::Arc;
use wasix_eth_core::mempool::mempool::Mempool;
use wasix_eth_storage::EthDatabase;
use wasix_eth_types::*;
use wasix_eth_core::chain_manager::NoopChainManager;
use tempfile::tempdir;
use tokio::sync::broadcast;
use wasix_eth_core::account_manager::AccountManager;
use wasix_eth_core::{Engine, EthConsensus};
use wasix_eth_execution::execution_provider::EthExecutionProvider;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{HeaderWriter, BlockWriter, MetadataWriter};

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

    let chain_config = ChainConfig {
        chain_id: 1,
        shanghai_time: Some(0),
        ..Default::default()
    };
    let config_json = serde_json::to_vec(&chain_config).unwrap();
    write_storage.set_metadata("chain_config".to_string(), Bytes::from(config_json)).unwrap();

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
async fn test_new_payload_state_root_mismatch_repro() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());

    // 1. Insert genesis block
    let genesis_header = Header {
        number: 0,
        parent_hash: B256::ZERO,
        beneficiary: Address::ZERO,
        state_root: EMPTY_ROOT_HASH,
        transactions_root: EMPTY_ROOT_HASH,
        receipts_root: EMPTY_ROOT_HASH,
        logs_bloom: Bloom::default(),
        difficulty: U256::ZERO,
        gas_limit: 30_000_000,
        gas_used: 0,
        timestamp: 1234,
        extra_data: Bytes::new(),
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(1_000_000_000),
        withdrawals_root: Some(EMPTY_ROOT_HASH),
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        ommers_hash: EMPTY_OMMER_ROOT_HASH,
        requests_hash: None,
    };
    let genesis_hash = genesis_header.hash_slow();
    
    writer.insert_header(0, genesis_header.clone()).unwrap();
    writer.set_canonical(0, genesis_hash).unwrap();
    writer.update_forkchoice(genesis_hash, Some(genesis_hash), Some(genesis_hash)).unwrap();

    let body = BlockBody {
        transactions: vec![],
        ommers: vec![],
        withdrawals: Some(vec![].into()),
    };
    writer.insert_block_body(genesis_hash, 0, body).unwrap();
    
    writer.set_metadata("chain_id".to_string(), 1u64.to_be_bytes().to_vec().into()).unwrap();
    writer.set_metadata("genesis_hash".to_string(), genesis_hash.as_slice().to_vec().into()).unwrap();

    // 2. Simulate block 1 building (simulation path)
    let forkchoice = ForkchoiceState {
        head_block_hash: genesis_hash,
        safe_block_hash: genesis_hash,
        finalized_block_hash: genesis_hash,
    };
    let attr = PayloadAttributes {
        timestamp: 1235,
        prev_randao: B256::repeat_byte(0x11),
        suggested_fee_recipient: Address::repeat_byte(0x22),
        withdrawals: Some(vec![].into()),
        parent_beacon_block_root: None,
    };

    let fc_result = engine.forkchoice_updated(forkchoice, Some(attr)).await.unwrap();
    let payload_id = fc_result.payload_id.expect("Should have payload_id");

    // 3. Get payload (simulation path)
    let payload_v1 = engine.get_payload_v1(payload_id).await.unwrap();
    let block_1_hash = payload_v1.block_hash;

    // 4. Submit block 1 via new_payload (validation path)
    let result = engine.new_payload(payload_v1.clone(), Some(vec![])).await.unwrap();
    
    if let PayloadStatusEnum::Invalid { validation_error } = &result.status {
        panic!("Block 1 is INVALID: {}", validation_error);
    }
    assert_eq!(result.status, PayloadStatusEnum::Valid, "Block 1 should be VALID");

    // 5. Update forkchoice to block 1
    let forkchoice_1 = ForkchoiceState {
        head_block_hash: block_1_hash,
        safe_block_hash: block_1_hash,
        finalized_block_hash: genesis_hash,
    };
    engine.forkchoice_updated(forkchoice_1, None).await.unwrap();

    // 5.5 Send a transaction to be included in block 2
    let tx_legacy = TxLegacy {
        chain_id: Some(1),
        nonce: 0,
        gas_price: 1_000_000_000,
        gas_limit: 21000,
        to: TxKind::Call(Address::repeat_byte(0x55)),
        value: U256::from(100),
        input: Bytes::new(),
    };
    let signature = Signature::test_signature();
    let tx = Transaction::Legacy(Signed::new_unchecked(tx_legacy, signature, B256::repeat_byte(0x66)));
    
    engine.submit_transaction(tx.clone()).await.unwrap();

    // 6. Simulate block 2 building
    let attr_2 = PayloadAttributes {
        timestamp: 1236,
        prev_randao: B256::repeat_byte(0x33),
        suggested_fee_recipient: Address::repeat_byte(0x44),
        withdrawals: Some(vec![].into()),
        parent_beacon_block_root: None,
    };
    let fc_result_2 = engine.forkchoice_updated(forkchoice_1, Some(attr_2)).await.unwrap();
    let payload_id_2 = fc_result_2.payload_id.expect("Should have payload_id_2");
    let payload_v1_2 = engine.get_payload_v1(payload_id_2).await.unwrap();

    // 7. Validate block 2 (validation path)
    let result_2 = engine.new_payload(payload_v1_2, Some(vec![])).await.unwrap();
    
    if let PayloadStatusEnum::Invalid { validation_error } = &result_2.status {
        panic!("Block 2 is INVALID: {}", validation_error);
    }
    assert_eq!(result_2.status, PayloadStatusEnum::Valid, "Block 2 should be VALID");
}
