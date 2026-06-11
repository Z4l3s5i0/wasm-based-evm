use wasix_eth_types::{Receipt, ConsensusReceipt, Eip658Value, B256, Bloom};
use serde_json;
use wasix_eth_utils::transaction_mapper::TransactionMapper;

#[test]
fn test_reproduce_zero_hash_issue() {
    let tx_hash = B256::from([0x42; 32]);
    let receipt = Receipt {
        tx_type: 0,
        receipt: ConsensusReceipt {
            status: Eip658Value::Eip658(true),
            cumulative_gas_used: 1000,
            logs: vec![],
        },
        logs_bloom: Bloom::ZERO,
    };
    
    // This calls the code we want to test
    let rpc_receipt = TransactionMapper::to_rpc_receipt(receipt, None, None, Some(tx_hash), 1000, None, None);
    
    println!("RPC Receipt hash: {:?}", rpc_receipt.transaction_hash);
    assert_eq!(rpc_receipt.transaction_hash, tx_hash, "Transaction hash should be set in the struct");

    // Now let's see the serialization
    let serialized = serde_json::to_value(&rpc_receipt).unwrap();
    println!("Serialized: {}", serialized);
    
    // Check if there are multiple transactionHash fields or something weird
    let obj = serialized.as_object().unwrap();
    for (k, v) in obj {
        if k.to_lowercase() == "transactionhash" {
            println!("Found key: {} = {}", k, v);
        }
    }
    
    let hash_in_json = serialized.get("transactionHash").and_then(|v| v.as_str());
    println!("Hash in JSON: {:?}", hash_in_json);
    
    assert!(hash_in_json.is_some(), "JSON should contain transactionHash");
    let expected_hash_str = format!("{:?}", tx_hash);
    assert_eq!(hash_in_json.unwrap(), expected_hash_str, "JSON hash should match");
}
