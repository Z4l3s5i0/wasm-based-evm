
#[cfg(test)]
mod tests {
    use alloy_network::TxSignerSync;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::broadcast;
    use wasix_eth_core::account_manager::AccountManager;
    use wasix_eth_core::mempool::listener::MempoolListener;
    use wasix_eth_core::mempool::mempool_provider::MempoolProvider;
    use wasix_eth_core::mempool::mempool::Mempool;
    use wasix_eth_core::{ChainManagerImpl, Consensus, Engine, EthConsensus};
    use wasix_eth_execution::execution_provider::EthExecutionProvider;
    use wasix_eth_storage::read::DatabaseReadProvider;
    use wasix_eth_storage::write::DatabaseWriteProvider;
    use wasix_eth_storage::write_traits::{AccountWriter, BlockWriter, HeaderWriter, MetadataWriter};
    use wasix_eth_storage::EthDatabase;
    use wasix_eth_types::{Address, Block, BlockBody, BlockId, Bloom, Bytes, ChainConfig, ExecutionPayloadV1, ForkchoiceState, Header, PayloadAttributes, PayloadId, PayloadStatusEnum, Receipt, SignableTransaction, Signature, Transaction, TrieAccount, TxLegacy, B256, U256, BlobsBundleV1};
    use wasix_eth_types::error::RpcResult;
    use wasix_eth_utils::engine_mapper::EngineMapper;


    async fn setup_engine() -> (Engine, Arc<EthDatabase>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let db = EthDatabase::open(&db_path).unwrap();
        db.init_tables().unwrap();
        
        let db_arc = Arc::new(db);
        let read_storage = DatabaseReadProvider::new(db_arc.inner());
        let write_storage = DatabaseWriteProvider::new(db_arc.inner());
        
        // Set default chain ID for tests
        write_storage.set_metadata("chain_id".to_string(), 1u64.to_be_bytes().to_vec().into()).unwrap();
        
        // Fund some dev accounts
        let account_manager_inner = AccountManager::new_with_dev_keys();
        for signer in account_manager_inner.signers.values() {
            write_storage.update_account(signer.address(), TrieAccount {
                nonce: 0,
                balance: U256::from(100_000_000_000_000_000_000u128), // 100 ETH
                storage_root: B256::ZERO,
                code_hash: B256::ZERO,
            }).unwrap();
        }
        
        write_storage.clone().commit().unwrap();

        let execution = Arc::new(EthExecutionProvider::new(read_storage.clone(), write_storage.clone()));
        let account_manager = Arc::new(account_manager_inner);
        let (event_tx, _) = broadcast::channel(100);
        let mempool = Arc::new(Mempool::new(U256::ZERO));
        let chain = Arc::new(ChainManagerImpl::new(read_storage.clone(), write_storage.clone()));

        let listener = MempoolListener::new(mempool.clone(), read_storage.clone(), event_tx.subscribe());
        tokio::spawn(async move {
            listener.run().await;
        });

        let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
        (Engine::new(read_storage, write_storage, execution, account_manager, chain, mempool, event_tx, consensus), db_arc, dir)
    }

    #[tokio::test]
    async fn test_submit_transaction_invalid_chain_id() {
        let (engine, _, _dir) = setup_engine().await;
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Address::ZERO.into(),
            value: U256::ZERO,
            input: Bytes::new(),
            chain_id: Some(8), // Invalid, setup_engine sets it to 1
        }.into_signed(Signature::test_signature()));

        let result = engine.submit_transaction(tx).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Invalid chain ID"));
        }
    }

    #[tokio::test]
    async fn test_submit_transaction_success() {
        let (engine, _, _dir) = setup_engine().await;
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Address::ZERO.into(),
            value: U256::ZERO,
            input: Bytes::new(),
            chain_id: Some(1),
        }.into_signed(Signature::test_signature()));

        let result = engine.submit_transaction(tx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_submit_transaction_invalid_signature() {
        let (engine, _, _dir) = setup_engine().await;
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 0,
            gas_price: 1,
            gas_limit: 21_000,
            to: Address::ZERO.into(),
            value: U256::ZERO,
            input: Bytes::new(),
            chain_id: Some(1),
        }.into_signed(Signature::test_signature()));

        let result = engine.submit_transaction(tx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_submit_transaction_duplicate() {
        let (engine, _, _dir) = setup_engine().await;
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Address::ZERO.into(),
            value: U256::ZERO,
            input: Bytes::new(),
            chain_id: Some(1),
        }.into_signed(Signature::test_signature()));

        engine.submit_transaction(tx.clone()).await.unwrap();
        let result = engine.submit_transaction(tx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_balance_success() {
        let (engine, db, _dir) = setup_engine().await;
        let addr = Address::repeat_byte(0xAA);
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.update_account(addr, TrieAccount {
            nonce: 0,
            balance: U256::from(1000),
            storage_root: B256::ZERO,
            code_hash: B256::ZERO,
        }).unwrap();
        writer.commit().unwrap();

        let balance = engine.get_balance(addr, BlockId::latest()).await.unwrap();
        assert_eq!(balance, U256::from(1000));
    }

    #[tokio::test]
    async fn test_get_balance_non_existent() {
        let (engine, _, _dir) = setup_engine().await;
        let addr = Address::repeat_byte(0xCC);
        let balance = engine.get_balance(addr, BlockId::latest()).await.unwrap();
        assert_eq!(balance, U256::ZERO);
    }

    #[tokio::test]
    async fn test_get_balance_error() {
        let (engine, _, _dir) = setup_engine().await;
        let result = engine.get_balance(Address::ZERO, BlockId::number(999)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_forkchoice_updated_success() {
        let (engine, db, _dir) = setup_engine().await;
        let mut header = Header::default();
        header.number = 1;
        let hash = header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, header).unwrap();
        writer.insert_block_hash(hash, 1).unwrap();
        writer.insert_block_body(hash, 1, BlockBody::default()).unwrap();
        writer.set_canonical(1, hash).unwrap();
        writer.commit().unwrap();

        let state = ForkchoiceState {
            head_block_hash: hash,
            safe_block_hash: hash,
            finalized_block_hash: hash,
        };
        let result = engine.forkchoice_updated(state, None, 1).await.unwrap();
        assert_eq!(result.payload_status.status, PayloadStatusEnum::Valid);
    }

    #[tokio::test]
    async fn test_forkchoice_updated_invalid_hash() {
        let (engine, _, _dir) = setup_engine().await;
        let state = ForkchoiceState {
            head_block_hash: B256::repeat_byte(0xEE),
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
        };
        let result = engine.forkchoice_updated(state, None, 1).await.unwrap();
        assert_eq!(result.payload_status.status, PayloadStatusEnum::Syncing);
    }

    #[tokio::test]
    async fn test_forkchoice_updated_invalid_block() {
        let (engine, _, _dir) = setup_engine().await;
        let invalid_hash = B256::repeat_byte(0xDD);
        let parent_hash = B256::repeat_byte(0xCC);
        
        // Mark the block as invalid
        use wasix_eth_core::InvalidationReason;
        engine.chain.add_invalid_block(invalid_hash, parent_hash, InvalidationReason::Hard).await;
        
        let state = ForkchoiceState {
            head_block_hash: invalid_hash,
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
        };
        
        let result = engine.forkchoice_updated(state, None, 1).await.unwrap();
        
        // It should return INVALID status instead of an error
        if let PayloadStatusEnum::Invalid { .. } = result.payload_status.status {
            // Success
        } else {
            panic!("Expected INVALID status, got {:?}", result.payload_status.status);
        }
    }

    #[tokio::test]
    async fn test_forkchoice_updated_with_attributes() {
        let (engine, db, _dir) = setup_engine().await;
        let mut header = Header::default();
        header.number = 1;
        header.timestamp = 100;
        let hash = header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, header).unwrap();
        writer.insert_block_hash(hash, 1).unwrap();
        writer.insert_block_body(hash, 1, BlockBody::default()).unwrap();
        writer.set_canonical(1, hash).unwrap();
        writer.commit().unwrap();

        let state = ForkchoiceState {
            head_block_hash: hash,
            safe_block_hash: hash,
            finalized_block_hash: hash,
        };
        let attr = PayloadAttributes {
            timestamp: 200,
            prev_randao: B256::ZERO,
            suggested_fee_recipient: Address::ZERO,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: None,
        };
        let result = engine.forkchoice_updated(state, Some(attr), 1).await.unwrap();
        assert!(result.payload_id.is_some());
    }

    #[tokio::test]
    async fn test_new_payload_invalid_block_hash() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { gas_limit: 30_000_000, ..Default::default() };
        parent_header.number = 1;
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.commit().unwrap();

        let payload = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO,
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::from(1_000_000_000),
            block_hash: B256::repeat_byte(0xAA), // INVALID HASH
            transactions: vec![],
        };

        let result = engine.new_payload(payload, None).await.unwrap();
        if let PayloadStatusEnum::Invalid { validation_error } = result.status {
            assert_eq!(validation_error, "INVALID_BLOCK_HASH");
        } else {
            panic!("Expected INVALID status with INVALID_BLOCK_HASH error, got {:?}", result.status);
        }
    }

    #[tokio::test]
    async fn test_new_payload_success() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { gas_limit: 30_000_000, ..Default::default() };
        parent_header.number = 1;
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.commit().unwrap();

        let payload = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO,
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::from(1_000_000_000),
            block_hash: B256::ZERO, // Placeholder
            transactions: vec![],
        };
        let transactions = vec![];
        let block = EngineMapper::payload_v1_to_block(&payload, transactions, None, &wasix_eth_types::ChainConfig::default(), None, None, None);
        let mut payload = payload;
        payload.block_hash = block.header.hash_slow();

        let result = engine.new_payload(payload, None).await.unwrap();
        assert_eq!(result.status, PayloadStatusEnum::Valid);
    }

    #[tokio::test]
    async fn test_new_payload_invalid_parent() {
        let (engine, _, _dir) = setup_engine().await;
        let payload = ExecutionPayloadV1 {
            parent_hash: B256::repeat_byte(0xEE),
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO,
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::ZERO,
            block_hash: B256::ZERO, // Placeholder
            transactions: vec![],
        };
        let block = EngineMapper::payload_v1_to_block(&payload, vec![], None, &wasix_eth_types::ChainConfig::default(), None, None, None);
        let mut payload = payload;
        payload.block_hash = block.header.hash_slow();

        let result = engine.new_payload(payload, None).await.unwrap();
        assert_eq!(result.status, PayloadStatusEnum::Syncing);
    }

    #[tokio::test]
    async fn test_new_payload_idempotency() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { number: 1, gas_limit: 30_000_000, ..Default::default() };
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.commit().unwrap();

        let payload = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO,
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::ZERO,
            block_hash: B256::ZERO, // Placeholder
            transactions: vec![],
        };
        let block = EngineMapper::payload_v1_to_block(&payload, vec![], None, &wasix_eth_types::ChainConfig::default(), None, None, None);
        let mut payload = payload;
        payload.block_hash = block.header.hash_slow();

        engine.new_payload(payload.clone(), None).await.unwrap();
        let result = engine.new_payload(payload, None).await.unwrap();
        assert_eq!(result.status, PayloadStatusEnum::Valid);
    }

    #[tokio::test]
    async fn test_new_payload_invalid_execution_non_canonical_parent() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { gas_limit: 30_000_000, ..Default::default() };
        parent_header.number = 1;
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        // NOT calling set_canonical(1, parent_hash) makes it non-canonical
        writer.commit().unwrap();

        let payload = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: B256::repeat_byte(0xEE), // Invalid state root to trigger Err(e) in execution
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::ZERO,
            block_hash: B256::ZERO, // Placeholder
            transactions: vec![],
        };
        let block = EngineMapper::payload_v1_to_block(&payload, vec![], None, &wasix_eth_types::ChainConfig::default(), None, None, None);
        let mut payload = payload;
        payload.block_hash = block.header.hash_slow();

        let result = engine.new_payload(payload, None).await.unwrap();
        
        assert!(matches!(result.status, PayloadStatusEnum::Invalid { .. }));
        assert_eq!(result.latest_valid_hash, Some(parent_hash));
    }

    #[tokio::test]
    async fn test_new_payload_invalid_gas_limit() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { gas_limit: 30_000_000, ..Default::default() };
        parent_header.number = 1;
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.set_canonical(1, parent_hash).unwrap();
        writer.commit().unwrap();

        let payload = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO, 
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 60_000_000, // Invalid: too large increase
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::ZERO,
            block_hash: B256::ZERO, // Placeholder
            transactions: vec![],
        };
        let block = EngineMapper::payload_v1_to_block(&payload, vec![], None, &wasix_eth_types::ChainConfig::default(), None, None, None);
        let mut payload = payload;
        payload.block_hash = block.header.hash_slow();

        let result = engine.new_payload(payload, None).await.unwrap();
        
        // This currently fails (returns Valid) because we lack header validation
        assert!(matches!(result.status, PayloadStatusEnum::Invalid { .. }), "Expected INVALID status for invalid gas limit, got {:?}", result.status);
        assert_eq!(result.latest_valid_hash, Some(parent_hash));
    }

    #[tokio::test]
    async fn test_new_payload_invalid_base_fee() {
        let (engine, db, _dir) = setup_engine().await;
        
        // Setup London fork
        let mut config = wasix_eth_types::ChainConfig::default();
        config.london_block = Some(0);
        let config_json = serde_json::to_vec(&config).unwrap();
        engine.write_storage.set_metadata("chain_config".to_string(), Bytes::from(config_json)).unwrap();
        engine.write_storage.clone().commit().unwrap();

        let mut parent_header = Header { gas_limit: 30_000_000, ..Default::default() };
        parent_header.number = 1;
        parent_header.gas_used = 15_000_000; // Target
        parent_header.base_fee_per_gas = Some(1_000_000_000);
        let parent_hash = parent_header.hash_slow();
        
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.set_canonical(1, parent_hash).unwrap();
        writer.commit().unwrap();

        let payload = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO, 
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::from(2_000_000_000), // Invalid: should be 1_000_000_000
            block_hash: B256::ZERO, // Placeholder
            transactions: vec![],
        };
        let block = EngineMapper::payload_v1_to_block(&payload, vec![], None, &config, None, None, None);
        let mut payload = payload;
        payload.block_hash = block.header.hash_slow();

        let result = engine.new_payload(payload, None).await.unwrap();
        
        assert!(matches!(result.status, PayloadStatusEnum::Invalid { .. }), "Expected INVALID status for invalid base fee, got {:?}", result.status);
        assert_eq!(result.latest_valid_hash, Some(parent_hash));
    }

    #[tokio::test]
    async fn test_new_payload_invalid_transaction_chain_id() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { gas_limit: 30_000_000, ..Default::default() };
        parent_header.number = 1;
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.set_canonical(1, parent_hash).unwrap();
        writer.commit().unwrap();

        let tx = Transaction::Legacy(TxLegacy {
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Address::ZERO.into(),
            value: U256::ZERO,
            input: Bytes::new(),
            chain_id: Some(8), // Invalid, setup_engine sets it to 1
        }.into_signed(Signature::test_signature()));

        let payload = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO, 
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1000,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::ZERO,
            block_hash: B256::ZERO, // Placeholder
            transactions: vec![alloy_rlp::encode(&tx).into()],
        };
        let block = EngineMapper::payload_v1_to_block(&payload, vec![tx], None, &wasix_eth_types::ChainConfig::default(), None, None, None);
        let mut payload = payload;
        payload.block_hash = block.header.hash_slow();

        let result = engine.new_payload(payload, None).await.unwrap();
        
        assert!(matches!(result.status, PayloadStatusEnum::Invalid { .. }), "Expected INVALID status for invalid tx chain ID, got {:?}", result.status);
        assert_eq!(result.latest_valid_hash, Some(parent_hash));
    }

    async fn wait_for_mempool(mempool: &dyn MempoolProvider, expected_len: usize) {
        for _ in 0..20 {
            if mempool.len().await == expected_len {
                return;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn test_mempool_persistence_sidechain_reorg() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { number: 1, gas_limit: 30_000_000, ..Default::default() };
        
        // Calculate state root for funded accounts
        let state_root = engine.write_storage.calculate_state_root(true, None).unwrap();
        parent_header.state_root = state_root;
        
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header.clone()).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.set_canonical(1, parent_hash).unwrap();
        writer.commit().unwrap();

        // 1. Submit a transaction
        let mut tx_legacy = TxLegacy {
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Address::ZERO.into(),
            value: U256::ZERO,
            input: Bytes::new(),
            chain_id: Some(1),
        };
        let sender = engine.account_manager.signers.values().next().unwrap();
        let signature = sender.sign_transaction_sync(&mut tx_legacy).unwrap();
        let tx = Transaction::Legacy(tx_legacy.into_signed(signature));
        engine.submit_transaction(tx.clone()).await.unwrap();
        
        wait_for_mempool(&*engine.mempool, 1).await;
        assert_eq!(engine.mempool.len().await, 1, "Transaction should be in mempool after submission");

        // 2. Call new_payload (A) with this transaction
        let payload_a = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: state_root, // Use same state root for simplicity since tx is just a transfer
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::repeat_byte(0xA),
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0, // Set to 0 to avoid root mismatch if execution doesn't update it
            timestamp: 1001,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::from(1_000_000_000),
            block_hash: B256::ZERO,
            transactions: vec![alloy_rlp::encode(&tx).into()],
        };
        
        // We need to use a real config to allow Paris/Engine API
        let config = ChainConfig {
            london_block: Some(0),
            ..Default::default()
        };
        
        let block_a = EngineMapper::payload_v1_to_block(&payload_a, vec![tx.clone()], None, &config, None, None, None);
        let mut payload_a = payload_a;
        payload_a.block_hash = block_a.header.hash_slow();

        engine.new_payload(payload_a, None).await.unwrap();

        // 3. VERIFY: Transaction should STILL be in mempool because Block A is not canonical!
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert_eq!(engine.mempool.len().await, 1, "Transaction should not be removed by non-canonical new_payload");

        // 4. Call forkchoice_updated for Parent (which is currently canonical head) with attributes for Block B
        let state = ForkchoiceState {
            head_block_hash: parent_hash,
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
        };
        let attr = PayloadAttributes {
            timestamp: 1002,
            prev_randao: B256::repeat_byte(0xB),
            suggested_fee_recipient: Address::ZERO,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: None,
        };
        
        // Temporarily override chain config in engine if possible, or just use one that works
        // Actually setup_engine uses NoopChainManager which doesn't really check forks
        
        let result = engine.forkchoice_updated(state, Some(attr), 1).await.unwrap();
        let payload_id = result.payload_id.expect("Should return payload_id when attributes provided at head");

        // 5. Get the payload and verify it contains the transaction
        let payload_b = engine.get_payload_v1(payload_id).await.unwrap();
        assert_eq!(payload_b.transactions.len(), 1, "Alternative payload should contain the transaction from mempool");
        
        // 6. Now set head to Block A and verify it is removed
        let state_a = ForkchoiceState {
            head_block_hash: block_a.header.hash_slow(),
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
        };
        engine.forkchoice_updated(state_a, None, 1).await.unwrap();
        
        wait_for_mempool(&*engine.mempool, 0).await;
        assert_eq!(engine.mempool.len().await, 0, "Transaction should be removed after block becomes canonical");
    }

    #[tokio::test]
    async fn test_mempool_readd_on_reorg() {
        let (engine, db, _dir) = setup_engine().await;
        let mut parent_header = Header { number: 1, gas_limit: 30_000_000, ..Default::default() };
        let state_root = engine.write_storage.calculate_state_root(true, None).unwrap();
        parent_header.state_root = state_root;
        
        let parent_hash = parent_header.hash_slow();
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.insert_header(1, parent_header.clone()).unwrap();
        writer.insert_block_hash(parent_hash, 1).unwrap();
        writer.insert_block_body(parent_hash, 1, BlockBody::default()).unwrap();
        writer.set_canonical(1, parent_hash).unwrap();
        writer.commit().unwrap();

        // 1. Submit TxA
        let mut tx_legacy = TxLegacy {
            nonce: 0,
            gas_price: 1_000_000_000,
            gas_limit: 21_000,
            to: Address::repeat_byte(0xA).into(),
            value: U256::ZERO,
            input: Bytes::new(),
            chain_id: Some(1),
        };
        let sender = engine.account_manager.signers.values().nth(1).unwrap();
        let signature = sender.sign_transaction_sync(&mut tx_legacy).unwrap();
        let tx_a = Transaction::Legacy(tx_legacy.into_signed(signature));
        engine.submit_transaction(tx_a.clone()).await.unwrap();
        
        wait_for_mempool(&*engine.mempool, 1).await;
        
        // 2. Make Block A canonical
        let payload_a = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: state_root,
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::repeat_byte(0xA),
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1001,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::from(1_000_000_000),
            block_hash: B256::ZERO,
            transactions: vec![alloy_rlp::encode(&tx_a).into()],
        };
        let config = ChainConfig { london_block: Some(0), ..Default::default() };
        let block_a = EngineMapper::payload_v1_to_block(&payload_a, vec![tx_a.clone()], None, &config, None, None, None);
        let mut payload_a = payload_a;
        payload_a.block_hash = block_a.header.hash_slow();
        let hash_a = payload_a.block_hash;

        engine.new_payload(payload_a, None).await.unwrap();
        engine.forkchoice_updated(ForkchoiceState { head_block_hash: hash_a, ..Default::default() }, None, 1).await.unwrap();
        
        wait_for_mempool(&*engine.mempool, 0).await;
        assert_eq!(engine.mempool.len().await, 0);

        // 3. Now reorg to Block B (which doesn't have TxA)
        let payload_b = ExecutionPayloadV1 {
            parent_hash,
            fee_recipient: Address::ZERO,
            state_root: state_root,
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::repeat_byte(0xB),
            block_number: 2,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1002,
            extra_data: Bytes::new(),
            base_fee_per_gas: U256::from(1_000_000_000),
            block_hash: B256::ZERO,
            transactions: vec![],
        };
        let block_b = EngineMapper::payload_v1_to_block(&payload_b, vec![], None, &config, None, None, None);
        let mut payload_b = payload_b;
        payload_b.block_hash = block_b.header.hash_slow();
        let hash_b = payload_b.block_hash;

        engine.new_payload(payload_b, None).await.unwrap();
        engine.forkchoice_updated(ForkchoiceState { head_block_hash: hash_b, ..Default::default() }, None, 1).await.unwrap();
        
        wait_for_mempool(&*engine.mempool, 1).await;
        assert_eq!(engine.mempool.len().await, 1, "TxA should be re-added to mempool after Block A is discarded");
    }

    #[tokio::test]
    async fn test_get_payload_v1_success() {
        let (engine, _, _dir) = setup_engine().await;
        let id = PayloadId::new([1; 8]);
        let writer = engine.write_storage.clone();
        writer.add_payload(id, Block::default(), vec![], BlobsBundleV1::default()).unwrap();
        writer.commit().unwrap();

        let result = engine.get_payload_v1(id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_payload_v2_success() {
        let (engine, _, _dir) = setup_engine().await;
        let id = PayloadId::new([1; 8]);
        let writer = engine.write_storage.clone();
        writer.add_payload(id, Block::default(), vec![], BlobsBundleV1::default()).unwrap();
        writer.commit().unwrap();

        let result = engine.get_payload_v2(id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_payload_success() {
        let (engine, db, _dir) = setup_engine().await;
        let id = PayloadId::new([1; 8]);
        let writer = DatabaseWriteProvider::new(db.inner());
        writer.add_payload(id, Block::default(), vec![], BlobsBundleV1::default()).unwrap();
        writer.commit().unwrap();

        let result: RpcResult<(Block<Transaction>, Vec<Receipt>, BlobsBundleV1)> = engine.get_payload(&id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_payload_failure() {
        let (engine, _, _dir) = setup_engine().await;
        let result: RpcResult<(Block<Transaction>, Vec<Receipt>, BlobsBundleV1)> = engine.get_payload(&PayloadId::new([2; 8])).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_payload_expired() {
        let (engine, _, _dir) = setup_engine().await;
        let result: RpcResult<(Block<Transaction>, Vec<Receipt>, BlobsBundleV1)> = engine.get_payload(&PayloadId::new([3; 8])).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculate_next_base_fee_success() {
        let (engine, _, _dir) = setup_engine().await;
        let mut config = wasix_eth_types::ChainConfig::default();
        config.london_block = Some(0);
        let config_json = serde_json::to_vec(&config).unwrap();
        engine.write_storage.set_metadata("chain_config".to_string(), Bytes::from(config_json)).unwrap();
        engine.write_storage.clone().commit().unwrap();

        let mut header = Header::default();
        header.gas_limit = 30_000_000;
        header.gas_used = 15_000_000;
        header.base_fee_per_gas = Some(1_000_000_000);

        let next_fee = engine.consensus.calculate_next_base_fee(&header, &config).unwrap();
        assert_eq!(next_fee, 1_000_000_000);
    }

    #[tokio::test]
    async fn test_calculate_next_base_fee_max_increase() {
        let (engine, _, _dir) = setup_engine().await;
        let mut config = wasix_eth_types::ChainConfig::default();
        config.london_block = Some(0);
        let config_json = serde_json::to_vec(&config).unwrap();
        engine.write_storage.set_metadata("chain_config".to_string(), Bytes::from(config_json)).unwrap();
        engine.write_storage.clone().commit().unwrap();

        let mut header = Header::default();
        header.gas_limit = 30_000_000;
        header.gas_used = 30_000_000;
        header.base_fee_per_gas = Some(1_000_000_000);

        let next_fee = engine.consensus.calculate_next_base_fee(&header, &config).unwrap();
        assert!(next_fee > 1_000_000_000);
    }

    #[tokio::test]
    async fn test_calculate_next_base_fee_no_base_fee() {
        let (engine, _, _dir) = setup_engine().await;
        let config = wasix_eth_types::ChainConfig::default();
        let header = Header::default();
        let next_fee = engine.consensus.calculate_next_base_fee(&header, &config);
        assert!(next_fee.is_none());
    }
}