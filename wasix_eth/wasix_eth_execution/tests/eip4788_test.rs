use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_types::*;
use wasix_eth_utils::info;
use tempfile::TempDir;
use wasix_eth_storage::read_traits::{AccountProvider, StorageProvider, BlockProvider};
use wasix_eth_storage::write_traits::{HeaderWriter};

#[tokio::test]
async fn test_eip4788_beacon_root_update() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_eip4788.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let mut config = genesis::GenesisConfiguration::default();
    config.config.chain_id = 1;
    config.config.terminal_total_difficulty = Some(U256::ZERO);
    config.config.shanghai_time = Some(0);
    config.config.cancun_time = Some(0); // Cancun active from genesis

    db.init_genesis(config).unwrap();

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());

    let execution = EthExecutionProvider::new(read_provider.clone(), write_provider.clone());

    let beacon_root = B256::repeat_byte(0x42);
    let timestamp = 0x1235u64;
    
    let mut block = Block::<Transaction>::default();
    block.header.number = 1;
    block.header.timestamp = timestamp;
    block.header.parent_hash = read_provider.block_hash(0).unwrap().unwrap();
    block.header.parent_beacon_block_root = Some(beacon_root);
    block.header.base_fee_per_gas = Some(1000000000);
    block.header.withdrawals_root = Some(alloy_trie::EMPTY_ROOT_HASH);
    block.header.state_root = B256::ZERO; // Enable building mode

    let (final_block, _) = execution.execute_block(block).expect("Execution failed");
    
    info!("Calculated state root for block 1: {:?}", final_block.header.state_root);
    
    let beacon_root_contract = Address::from([
        0x00, 0x0F, 0x3d, 0xf6, 0xD7, 0x32, 0x80, 0x7E, 0xf1, 0x31, 
        0x9f, 0xB7, 0xB8, 0xbB, 0x85, 0x22, 0xd0, 0xBe, 0xac, 0x02
    ]);

    // Check storage
    let index = timestamp % 8191;
    let timestamp_slot = B256::from(U256::from(index));
    let root_slot = B256::from(U256::from(index + 8191));
    
    let stored_timestamp = read_provider.storage(beacon_root_contract, timestamp_slot, None).unwrap();
    let stored_root = read_provider.storage(beacon_root_contract, root_slot, None).unwrap();
    
    assert_eq!(stored_timestamp, U256::from(timestamp));
    assert_eq!(stored_root, U256::from_be_bytes(beacon_root.0));
    
    // Also check that the contract was created with code
    let account = read_provider.account(beacon_root_contract, None).unwrap().expect("Contract should exist");
    assert_ne!(account.code_hash, alloy_primitives::KECCAK256_EMPTY);
    assert_eq!(account.nonce, 1);

    // Now execute block 2
    let beacon_root_2 = B256::repeat_byte(0x43);
    let timestamp_2 = 0x1236u64;

    let mut block_2 = Block::<Transaction>::default();
    block_2.header.number = 2;
    block_2.header.timestamp = timestamp_2;
    block_2.header.parent_hash = final_block.header.hash_slow();
    block_2.header.parent_beacon_block_root = Some(beacon_root_2);
    block_2.header.base_fee_per_gas = Some(1000000000);
    block_2.header.withdrawals_root = Some(alloy_trie::EMPTY_ROOT_HASH);
    block_2.header.state_root = B256::ZERO;

    let (final_block_2, _) = execution.execute_block(block_2.clone()).expect("Execution failed for block 2");
    
    info!("Calculated state root for block 2: {:?}", final_block_2.header.state_root);

    // Check storage for block 2
    let index_2 = timestamp_2 % 8191;
    let timestamp_slot_2 = B256::from(U256::from(index_2));
    let root_slot_2 = B256::from(U256::from(index_2 + 8191));

    // Verify storage in latest state
    let stored_timestamp_2 = read_provider.storage(beacon_root_contract, timestamp_slot_2, None).unwrap();
    let stored_root_2 = read_provider.storage(beacon_root_contract, root_slot_2, None).unwrap();

    assert_eq!(stored_timestamp_2, U256::from(timestamp_2), "Timestamp 2 should be stored");
    assert_eq!(stored_root_2, U256::from_be_bytes(beacon_root_2.0), "Beacon root 2 should be stored");

    // Verify it is also readable using block 2's state root
    let stored_timestamp_2_sr = read_provider.storage(beacon_root_contract, timestamp_slot_2, Some(final_block_2.header.state_root)).unwrap();
    assert_eq!(stored_timestamp_2_sr, U256::from(timestamp_2), "Timestamp 2 should be stored (via state root)");

    // Verify Block 1's storage is still there (if we correctly chain blocks)
    // For this to work in this test, we MUST insert block 1 into the DB so block 2 can find its parent
    let batch = write_provider.begin_batch().unwrap();
    batch.insert_header(final_block.header.hash_slow(), final_block.header.clone()).unwrap();
    batch.commit().unwrap();

    // Now execute block 2 AGAIN, it should now correctly find parent state
    let (final_block_2_chained, _) = execution.execute_block(block_2).expect("Execution failed for block 2 chained");
    info!("Calculated state root for block 2 (chained): {:?}", final_block_2_chained.header.state_root);

    let stored_timestamp_1_from_2 = read_provider.storage(beacon_root_contract, timestamp_slot, Some(final_block_2_chained.header.state_root)).unwrap();
    assert_eq!(stored_timestamp_1_from_2, U256::from(timestamp), "Block 1 timestamp should be preserved in Block 2");
}
