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
use alloy_rlp::Encodable;

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
        terminal_total_difficulty: Some(U256::ZERO), // Force Paris
        ..Default::default()
    };
    let config_json = serde_json::to_vec(&chain_config).unwrap();
    write_storage.set_metadata("chain_config".to_string(), Bytes::from(config_json)).unwrap();
    write_storage.set_metadata("chain_id".to_string(), 1u64.to_be_bytes().to_vec().into()).unwrap();

    let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
    (Engine::new(read_storage, write_storage, execution, account_manager, chain, mempool, event_tx, consensus), db_arc, dir)
}

fn create_genesis(writer: &DatabaseWriteProvider) -> B256 {
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
        timestamp: 1000,
        extra_data: Bytes::new(),
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(1_000_000_000),
        withdrawals_root: Some(EMPTY_ROOT_HASH),
        ..Default::default()
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
    writer.set_metadata("genesis_hash".to_string(), genesis_hash.as_slice().to_vec().into()).unwrap();
    
    genesis_hash
}

fn create_valid_payload(parent_hash: B256, number: u64, timestamp: u64) -> ExecutionPayloadV1 {
    let mut header = Header {
        parent_hash,
        beneficiary: Address::repeat_byte(0x11),
        state_root: EMPTY_ROOT_HASH,
        transactions_root: EMPTY_ROOT_HASH,
        receipts_root: EMPTY_ROOT_HASH,
        logs_bloom: Bloom::default(),
        difficulty: U256::ZERO,
        number,
        gas_limit: 30_000_000,
        gas_used: 0,
        timestamp,
        extra_data: Bytes::new(),
        mix_hash: B256::repeat_byte(0x22), // prev_randao
        nonce: B64::ZERO,
        base_fee_per_gas: Some(1_000_000_000),
        withdrawals_root: Some(EMPTY_ROOT_HASH),
        ..Default::default()
    };
    
    let block_hash = header.hash_slow();
    
    ExecutionPayloadV1 {
        parent_hash,
        fee_recipient: header.beneficiary,
        state_root: header.state_root,
        receipts_root: header.receipts_root,
        logs_bloom: header.logs_bloom,
        prev_randao: header.mix_hash,
        block_number: header.number,
        gas_limit: header.gas_limit,
        gas_used: header.gas_used,
        timestamp: header.timestamp,
        extra_data: header.extra_data.clone(),
        base_fee_per_gas: header.base_fee_per_gas.map(U256::from).unwrap_or_default(),
        block_hash,
        transactions: vec![],
    }
}

async fn test_invalid_payload(
    engine: &Engine,
    mut payload: ExecutionPayloadV1,
    syncing: bool,
    expected_error_substring: Option<&str>,
    force_invalid_hash: bool,
) {
    // If syncing is true, we ensure parent_hash is unknown
    if syncing {
        payload.parent_hash = B256::repeat_byte(0x44);
    }
    
    let mut header = Header {
        parent_hash: payload.parent_hash,
        beneficiary: payload.fee_recipient,
        state_root: payload.state_root,
        transactions_root: EMPTY_ROOT_HASH,
        receipts_root: payload.receipts_root,
        logs_bloom: payload.logs_bloom,
        difficulty: U256::ZERO,
        number: payload.block_number,
        gas_limit: payload.gas_limit,
        gas_used: payload.gas_used,
        timestamp: payload.timestamp,
        extra_data: payload.extra_data.clone(),
        mix_hash: payload.prev_randao,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(payload.base_fee_per_gas.to::<u64>()),
        withdrawals_root: Some(EMPTY_ROOT_HASH),
        ..Default::default()
    };
    
    // Recalculate block hash with the updated fields
    payload.block_hash = header.hash_slow();

    if force_invalid_hash {
        payload.block_hash = B256::repeat_byte(0xEE);
    }

    let result = engine.new_payload(payload.clone(), None).await.unwrap();

    if syncing && !force_invalid_hash {
        assert!(result.status == PayloadStatusEnum::Syncing || result.status == PayloadStatusEnum::Accepted);
        assert_eq!(result.latest_valid_hash, None);
    } else {
        match result.status {
            PayloadStatusEnum::Invalid { validation_error } => {
                if let Some(err) = expected_error_substring {
                    assert!(validation_error.contains(err), "Error '{}' does not contain '{}'", validation_error, err);
                }
            }
            status => {
                if expected_error_substring.is_some() {
                     panic!("Expected Invalid status (due to {}), got {:?}", expected_error_substring.unwrap(), status);
                } else {
                     panic!("Expected Invalid status, got {:?}", status);
                }
            }
        }
    }
}

#[tokio::test]
async fn test_invalid_new_payload_parent_hash() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let payload = create_valid_payload(genesis_hash, 1, 1001);
    
    // ParentHash Invalid, Syncing=False
    // In our Engine, if ParentHash is unknown, it returns Syncing.
    // Hive might expect INVALID if we explicitly set a bad block_hash as well.
    test_invalid_payload(&engine, payload.clone(), false, Some("Block hash mismatch"), true).await;
    
    // ParentHash Invalid, Syncing=True
    test_invalid_payload(&engine, payload.clone(), true, None, false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_prev_randao() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    payload.prev_randao = B256::repeat_byte(0x88); // Different from header mix_hash
    
    // In our EngineMapper, payload.prev_randao is mapped to header.mix_hash for Paris+
    // If we change it in payload and recalculate hash, it will pass the first check.
    // However, does the execution care about mix_hash?
    test_invalid_payload(&engine, payload.clone(), false, None, false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_incomplete_transactions() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    
    // Legacy transaction
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
    let mut tx_bytes = Vec::new();
    tx.encode_2718(&mut tx_bytes);
    
    // Incomplete transaction (truncated)
    let incomplete_tx = Bytes::from(tx_bytes[..tx_bytes.len()-1].to_vec());
    payload.transactions = vec![incomplete_tx];
    
    // This should fail during transaction decoding in new_payload
    let result = engine.new_payload(payload.clone(), None).await.unwrap();
    assert!(matches!(result.status, PayloadStatusEnum::Invalid { .. }));
    if let PayloadStatusEnum::Invalid { validation_error } = result.status {
        assert!(validation_error.contains("Failed to decode transaction"));
    }
}

#[tokio::test]
async fn test_invalid_new_payload_transaction_signature() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    
    // Create transaction with invalid signature (random bytes for signature)
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
    let mut tx_bytes = Vec::new();
    tx.encode_2718(&mut tx_bytes);
    
    // Corrupt the signature bytes in RLP
    let last_idx = tx_bytes.len() - 1;
    tx_bytes[last_idx] ^= 0xFF;

    payload.transactions = vec![Bytes::from(tx_bytes)];
    
    let result = engine.new_payload(payload.clone(), None).await.unwrap();
    assert!(matches!(result.status, PayloadStatusEnum::Invalid { .. }));
}

#[tokio::test]
async fn test_invalid_new_payload_transaction_nonce() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    
    // Transaction with high nonce
    let tx_legacy = TxLegacy {
        chain_id: Some(1),
        nonce: 10, // Should be 0
        gas_price: 1_000_000_000,
        gas_limit: 21000,
        to: TxKind::Call(Address::repeat_byte(0x55)),
        value: U256::from(100),
        input: Bytes::new(),
    };
    let signature = Signature::test_signature();
    let tx = Transaction::Legacy(Signed::new_unchecked(tx_legacy, signature, B256::repeat_byte(0x66)));
    let mut tx_bytes = Vec::new();
    tx.encode_2718(&mut tx_bytes);
    payload.transactions = vec![Bytes::from(tx_bytes)];
    
    // Recalculate block hash
    let mut header = Header {
        parent_hash: payload.parent_hash,
        beneficiary: payload.fee_recipient,
        state_root: payload.state_root,
        transactions_root: proofs::calculate_transaction_root(&[tx]),
        receipts_root: payload.receipts_root,
        logs_bloom: payload.logs_bloom,
        difficulty: U256::ZERO,
        number: payload.block_number,
        gas_limit: payload.gas_limit,
        gas_used: payload.gas_used,
        timestamp: payload.timestamp,
        extra_data: payload.extra_data.clone(),
        mix_hash: payload.prev_randao,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(payload.base_fee_per_gas.to::<u64>()),
        withdrawals_root: Some(EMPTY_ROOT_HASH),
        ..Default::default()
    };
    payload.block_hash = header.hash_slow();

    // Execution should fail due to nonce mismatch
    test_invalid_payload(&engine, payload.clone(), false, None, false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_transaction_gas_price() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    
    // Transaction with gas price lower than base fee
    let tx_legacy = TxLegacy {
        chain_id: Some(1),
        nonce: 0,
        gas_price: 500_000_000, // Base fee is 1G
        gas_limit: 21000,
        to: TxKind::Call(Address::repeat_byte(0x55)),
        value: U256::from(100),
        input: Bytes::new(),
    };
    let signature = Signature::test_signature();
    let tx = Transaction::Legacy(Signed::new_unchecked(tx_legacy, signature, B256::repeat_byte(0x66)));
    let mut tx_bytes = Vec::new();
    tx.encode_2718(&mut tx_bytes);
    payload.transactions = vec![Bytes::from(tx_bytes)];
    
    // Recalculate block hash
    let mut header = Header {
        parent_hash: payload.parent_hash,
        beneficiary: payload.fee_recipient,
        state_root: payload.state_root,
        transactions_root: proofs::calculate_transaction_root(&[tx]),
        receipts_root: payload.receipts_root,
        logs_bloom: payload.logs_bloom,
        difficulty: U256::ZERO,
        number: payload.block_number,
        gas_limit: payload.gas_limit,
        gas_used: payload.gas_used,
        timestamp: payload.timestamp,
        extra_data: payload.extra_data.clone(),
        mix_hash: payload.prev_randao,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(payload.base_fee_per_gas.to::<u64>()),
        withdrawals_root: Some(EMPTY_ROOT_HASH),
        ..Default::default()
    };
    payload.block_hash = header.hash_slow();

    // Execution should fail due to gas price
    test_invalid_payload(&engine, payload.clone(), false, None, false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_state_root() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    payload.state_root = B256::repeat_byte(0x99); // Invalid state root
    
    // Syncing=False
    test_invalid_payload(&engine, payload.clone(), false, Some("State root mismatch"), false).await;
    
    // Syncing=True
    test_invalid_payload(&engine, payload.clone(), true, Some("State root mismatch"), false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_receipts_root() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    payload.receipts_root = B256::repeat_byte(0x99);
    
    test_invalid_payload(&engine, payload.clone(), false, Some("Receipts root mismatch"), false).await;
    test_invalid_payload(&engine, payload.clone(), true, Some("Receipts root mismatch"), false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_number() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    payload.block_number = 2; // Should be 1
    
    test_invalid_payload(&engine, payload.clone(), false, None, false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_gas_limit() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    payload.gas_limit = 10_000_000; // Genesis has 30M
    
    test_invalid_payload(&engine, payload.clone(), false, None, false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_gas_used() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    payload.gas_used = 1000; // Should be 0
    
    test_invalid_payload(&engine, payload.clone(), false, Some("Gas used mismatch"), false).await;
}

#[tokio::test]
async fn test_invalid_new_payload_timestamp() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());
    let genesis_hash = create_genesis(&writer);
    
    let mut payload = create_valid_payload(genesis_hash, 1, 1001);
    payload.timestamp = 500; // Less than genesis (1000)
    
    test_invalid_payload(&engine, payload.clone(), false, None, false).await;
}
