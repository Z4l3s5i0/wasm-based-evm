// use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider}; // Removed unused import
use wasix_eth_storage::EthDatabase;
// use wasix_eth_storage::read::DatabaseReadProvider; // Removed unused import
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_types::*;
use tempfile::TempDir;
use alloy_primitives::B256;
use sha2::{Sha256, Digest};

#[tokio::test]
async fn test_eip7685_requests_hash() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_eip7685.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let write_provider = DatabaseWriteProvider::new(db.inner());
    let batch = write_provider.begin_batch().unwrap();
    let processor = wasix_eth_execution::block::BlockProcessor::new(&batch);

    // Test cases for EIP-7685 requests_hash
    // Spec: requests_hash = sha256(sha256(r1) ++ sha256(r2) ++ ...)
    // Ordered by type, skipping len <= 1.

    // Case 1: Empty requests
    let requests: Vec<Vec<u8>> = vec![];
    let hash = processor.calculate_requests_hash(Hardfork::Prague, &requests);
    assert_eq!(hash, constants::EMPTY_REQUESTS_HASH);

    // Case 2: Requests with len <= 1 should be skipped
    let requests = vec![vec![0x00], vec![0x01]];
    let hash = processor.calculate_requests_hash(Hardfork::Prague, &requests);
    assert_eq!(hash, constants::EMPTY_REQUESTS_HASH);

    // Case 3: Single request
    let r1 = vec![0x00, 0x01, 0x02, 0x03];
    let requests = vec![r1.clone()];
    let hash = processor.calculate_requests_hash(Hardfork::Prague, &requests);
    
    let expected_r1_hash = Sha256::digest(&r1);
    let expected_final_hash = Sha256::digest(&expected_r1_hash);
    assert_eq!(hash, B256::from_slice(&expected_final_hash));

    // Case 4: Multiple requests of same type
    let r1 = vec![0x00, 0x01, 0x02];
    let r2 = vec![0x00, 0x03, 0x04];
    let requests = vec![r1.clone(), r2.clone()];
    let hash = processor.calculate_requests_hash(Hardfork::Prague, &requests);
    
    let mut intermediate = Vec::new();
    intermediate.extend_from_slice(&Sha256::digest(&r1));
    intermediate.extend_from_slice(&Sha256::digest(&r2));
    let expected_final_hash = Sha256::digest(&intermediate);
    assert_eq!(hash, B256::from_slice(&expected_final_hash));

    // Case 5: Multiple request types, should be ordered by type
    let r0 = vec![0x00, 0xAA];
    let r1 = vec![0x01, 0xBB];
    let r2 = vec![0x02, 0xCC];
    
    // Pass them out of order
    let requests = vec![r1.clone(), r2.clone(), r0.clone()];
    let hash = processor.calculate_requests_hash(Hardfork::Prague, &requests);
    
    let mut intermediate = Vec::new();
    intermediate.extend_from_slice(&Sha256::digest(&r0));
    intermediate.extend_from_slice(&Sha256::digest(&r1));
    intermediate.extend_from_slice(&Sha256::digest(&r2));
    let expected_final_hash = Sha256::digest(&intermediate);
    assert_eq!(hash, B256::from_slice(&expected_final_hash));
    
    // Case 6: Pre-Prague, should NOT be ordered by type
    // Pass them out of order
    let requests = vec![r1.clone(), r2.clone(), r0.clone()];
    let hash = processor.calculate_requests_hash(Hardfork::Cancun, &requests);
    
    let mut intermediate_no_sort = Vec::new();
    intermediate_no_sort.extend_from_slice(&Sha256::digest(&r1));
    intermediate_no_sort.extend_from_slice(&Sha256::digest(&r2));
    intermediate_no_sort.extend_from_slice(&Sha256::digest(&r0));
    let expected_final_hash_no_sort = Sha256::digest(&intermediate_no_sort);
    assert_eq!(hash, B256::from_slice(&expected_final_hash_no_sort));
    assert_ne!(hash, B256::from_slice(&expected_final_hash)); // Should be different from Case 5

    println!("EIP-7685 requests_hash test passed!");
}
