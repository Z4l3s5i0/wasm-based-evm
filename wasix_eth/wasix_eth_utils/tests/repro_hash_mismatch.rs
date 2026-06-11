use wasix_eth_types::{ExecutionPayloadV1, Transaction, B256, U256, Bloom, hex};
use wasix_eth_utils::engine_mapper::EngineMapper;

#[test]
fn test_block_hash_repro_paris() {
    // Data from Hive log line 13
    let payload = ExecutionPayloadV1 {
        parent_hash: B256::from_slice(&hex::decode("d462b6793c2895fd61cd63e098874774d3e03556e14ec40fb9861956e39eb8b0").unwrap()),
        fee_recipient: "0000000000000000000000000000000000000000".parse().unwrap(),
        state_root: B256::from_slice(&hex::decode("0000000000000000000000000000000000000000000000000000000000000000").unwrap()),
        receipts_root: B256::from_slice(&hex::decode("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421").unwrap()),
        logs_bloom: Bloom::ZERO,
        prev_randao: B256::from_slice(&hex::decode("e9e7dbb1818cf9fc9c0e46e97b32b1b667f17a0e02f7b0616c6c2007e7fab57b").unwrap()),
        block_number: 1,
        gas_limit: 0x2fefd8,
        gas_used: 0,
        timestamp: 0x1235,
        extra_data: hex::decode("").unwrap().into(),
        base_fee_per_gas: U256::from(0x342770c0u64),
        block_hash: B256::from_slice(&hex::decode("33c0d19d41b25b77ea855b665c243abcdcd4a81e2ad6b530890b890b4c9d95ce").unwrap()),
        transactions: vec![],
    };

    let transactions: Vec<Transaction> = vec![];
    let withdrawals = None;
    let chain_config = wasix_eth_types::ChainConfig::default();

    let block = EngineMapper::payload_v1_to_block(&payload, transactions, withdrawals, &chain_config, None, None, None);
    let actual_hash = block.header.hash_slow();

    println!("Expected block hash: {:?}", payload.block_hash);
    println!("Actual block hash:   {:?}", actual_hash);
    
    // Also print some header fields to see if they are what we expect
    println!("Header: {:?}", block.header);
    println!("RLP Header: {}", hex::encode(alloy_rlp::encode(&block.header)));

    assert_eq!(actual_hash, payload.block_hash, "Block hash mismatch");
}
