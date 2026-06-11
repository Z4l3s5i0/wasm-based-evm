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
    (Engine::new(read_storage, write_storage, execution, account_manager, chain, mempool, event_tx, consensus), db_arc, dir)
}

#[tokio::test]
async fn test_multiple_new_payloads_extending_canonical_chain() {
    let (engine, db, _dir) = setup_engine().await;
    let writer = DatabaseWriteProvider::new(db.inner());

    // 1. Insert genesis block
    let genesis_header = Header {
        number: 0,
        parent_hash: B256::ZERO,
        beneficiary: Address::ZERO,
        state_root: hex!("dac58864b1d70d65174a6ead9c462edc165ca9f05e32778b64eb7059a789ec74").into(),
        transactions_root: EMPTY_ROOT_HASH,
        receipts_root: EMPTY_ROOT_HASH,
        logs_bloom: Bloom::default(),
        difficulty: U256::from(0x30000),
        gas_limit: 0x2fefd8,
        gas_used: 0,
        timestamp: 0x1234,
        extra_data: Bytes::from(hex!("0000000000000000000000000000000000000000000000000000000000000000658bdf435d810c91414ec09147daa6db624063790000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000")),
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(0x3b9aca00),
        withdrawals_root: None,
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        ommers_hash: EMPTY_OMMER_ROOT_HASH,
        requests_hash: None,
    };
    let genesis_hash = genesis_header.hash_slow();
    
    writer.insert_header(0, genesis_header.clone()).unwrap();
    writer.set_canonical(0, genesis_hash).unwrap();
    writer.update_forkchoice(genesis_hash, Some(B256::ZERO), Some(B256::ZERO)).unwrap();

    let body = BlockBody {
        transactions: vec![],
        ommers: vec![],
        withdrawals: None,
    };
    writer.insert_block_body(genesis_hash, 0, body).unwrap();
    
    writer.set_metadata("chain_id".to_string(), 1u64.to_be_bytes().to_vec().into()).unwrap();
    writer.set_metadata("genesis_hash".to_string(), genesis_hash.as_slice().to_vec().into()).unwrap();

    let mut current_head = genesis_hash;

    // Produce blocks 1 to 5
    for i in 1..=5 {
        let timestamp = 0x1234 + i;
        let prev_randao = B256::repeat_byte(i as u8); // In logs it varies more but this should be fine
        
        let forkchoice = ForkchoiceState {
            head_block_hash: current_head,
            safe_block_hash: if i > 1 { current_head } else { B256::ZERO },
            finalized_block_hash: if i > 2 { genesis_hash } else { B256::ZERO },
        };
        
        let attr = PayloadAttributes {
            timestamp,
            prev_randao,
            suggested_fee_recipient: Address::ZERO,
            withdrawals: None,
            parent_beacon_block_root: None,
        };

        let fc_result = engine.forkchoice_updated(forkchoice, Some(attr)).await.unwrap();
        let payload_id = fc_result.payload_id.expect("Should have payload_id");
        let payload = engine.get_payload_v1(payload_id).await.unwrap();
        
        let result = engine.new_payload(payload.clone(), None).await.unwrap();
        assert_eq!(result.status, PayloadStatusEnum::Valid, "Block {} should be VALID", i);
        
        current_head = payload.block_hash;
        let fc_update = ForkchoiceState {
            head_block_hash: current_head,
            safe_block_hash: if i > 0 { current_head } else { B256::ZERO },
            finalized_block_hash: if i > 1 { genesis_hash } else { B256::ZERO },
        };
        engine.forkchoice_updated(fc_update, None).await.unwrap();
    }

    // After block 5, send a transaction
    // Transaction from logs: 0xf865808506fc23ac00830124f894ccee97ac5fd4ed394bd97a7e89b78ecc8a6784d3808032a0950e0535d562370312f2a57ebf3a6e398ff263c9ed782d8ff79ac74ab44f622fa022ac9cfab2a17cc3205c82cd7ea59ccaf879d3e4d4516f63595e3069b5ec5f27
    let tx_bytes = hex!("f865808506fc23ac00830124f894ccee97ac5fd4ed394bd97a7e89b78ecc8a6784d3808032a0950e0535d562370312f2a57ebf3a6e398ff263c9ed782d8ff79ac74ab44f622fa022ac9cfab2a17cc3205c82cd7ea59ccaf879d3e4d4516f63595e3069b5ec5f27");
    let tx: Transaction = alloy_rlp::Decodable::decode(&mut &tx_bytes[..]).unwrap();
    engine.submit_transaction(tx).await.unwrap();

    // Produce block 6 - Payload 1
    let attr_6a = PayloadAttributes {
        timestamp: 0x123a,
        prev_randao: hex!("cecff1fc1797806cec8a9a905cde5d15c2667e31dc971a43d0fb309c95d78743").into(),
        suggested_fee_recipient: Address::ZERO,
        withdrawals: None,
        parent_beacon_block_root: None,
    };
    let fc_state_5 = ForkchoiceState {
        head_block_hash: current_head,
        safe_block_hash: current_head, // Adjust according to logs if needed
        finalized_block_hash: current_head, // Adjust according to logs if needed
    };
    // Logs for ID 21: head=block5, safe=block4, finalized=block3
    // But my loop might have different hashes.
    
    let fc_result_6a = engine.forkchoice_updated(ForkchoiceState {
        head_block_hash: current_head,
        safe_block_hash: current_head,
        finalized_block_hash: genesis_hash,
    }, Some(attr_6a)).await.unwrap();
    let payload_id_6a = fc_result_6a.payload_id.expect("Should have payload_id_6a");
    let payload_6a = engine.get_payload_v1(payload_id_6a).await.unwrap();
    
    let result_6a = engine.new_payload(payload_6a, None).await.unwrap();
    assert_eq!(result_6a.status, PayloadStatusEnum::Valid, "Block 6a should be VALID");

    // Produce block 6 - Payload 2 (Different prev_randao)
    let attr_6b = PayloadAttributes {
        timestamp: 0x123a,
        prev_randao: hex!("2f680578e13e441007120a83f1104091ad6ed36216bbb8e5bd26d4eb4dd59aaa").into(),
        suggested_fee_recipient: Address::ZERO,
        withdrawals: None,
        parent_beacon_block_root: None,
    };
    
    // We don't necessarily need to call forkchoice_updated again if we just want to test new_payload
    // But the logs show ID 24 is a new_payload for a DIFFERENT block 6.
    // In the logs, ID 24 newPayload has:
    // prevRandao: 0x2f680578e13e441007120a83f1104091ad6ed36216bbb8e5bd26d4eb4dd59aaa
    // blockNumber: 0x6
    // blockHash: 0x8204883c5ef56275a7617a393639688c13b48646c80033387cdadc16421e098d
    
    // Let's get this second payload too
    let fc_result_6b = engine.forkchoice_updated(ForkchoiceState {
        head_block_hash: current_head,
        safe_block_hash: current_head,
        finalized_block_hash: genesis_hash,
    }, Some(attr_6b)).await.unwrap();
    let payload_id_6b = fc_result_6b.payload_id.expect("Should have payload_id_6b");
    let payload_6b = engine.get_payload_v1(payload_id_6b).await.unwrap();
    
    let result_6b = engine.new_payload(payload_6b, None).await.unwrap();
    assert_eq!(result_6b.status, PayloadStatusEnum::Valid, "Block 6b should be VALID");
}
