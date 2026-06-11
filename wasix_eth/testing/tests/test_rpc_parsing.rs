use wasix_eth_types::B256;
use serde_json::json;

#[test]
fn test_b256_deserialization() {
    // Correct hash with 0x prefix
    let json_correct = json!("0x1000000000000000000000000000000000000000000000000000000000000001");
    let res_correct: Result<B256, _> = serde_json::from_value(json_correct);
    assert!(res_correct.is_ok());

    // Hash without 0x prefix - this is what the Hive test sends
    let json_invalid = json!("1000000000000000000000000000000000000000000000000000000000000001");

    // Test serde_json::from_value
    let res_invalid: Result<B256, _> = serde_json::from_value(json_invalid.clone());
    println!("serde_json::from_value without 0x: {:?}", res_invalid);

    // Test direct parsing if any
    let s = json_invalid.as_str().unwrap();
    let res_parse = s.parse::<B256>();
    println!("s.parse::<B256>() without 0x: {:?}", res_parse);

    // Test BlockId
    let json_block_hex = json!("1");
    let res_block: Result<wasix_eth_types::BlockId, _> = serde_json::from_value(json_block_hex);
    println!("BlockId from \"1\": {:?}", res_block);

    // Test Address
    let json_addr = json!("1000000000000000000000000000000000000001");
    let res_addr: Result<wasix_eth_types::Address, _> = serde_json::from_value(json_addr);
    println!("Address from \"1000...001\" (no 0x): {:?}", res_addr);
}