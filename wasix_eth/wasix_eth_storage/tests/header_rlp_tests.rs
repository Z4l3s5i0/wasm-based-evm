use wasix_eth_types::*;
use alloy_rlp::{Encodable, Decodable};
use alloy_primitives::{B256, U256, B64};

#[test]
fn test_header_rlp_cancun() {
    let header = Header {
        number: 0,
        timestamp: 420,
        gas_limit: 30_000_000,
        state_root: B256::ZERO,
        beneficiary: Address::ZERO,
        difficulty: U256::ZERO,
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(1_000_000_000),
        extra_data: Bytes::from(vec![0x68, 0x69, 0x76, 0x65, 0x63, 0x68, 0x61, 0x69, 0x6e]),
        transactions_root: alloy_trie::EMPTY_ROOT_HASH,
        receipts_root: alloy_trie::EMPTY_ROOT_HASH,
        withdrawals_root: Some(alloy_trie::EMPTY_ROOT_HASH),
        gas_used: 0,
        parent_hash: B256::ZERO,
        ommers_hash: EMPTY_OMMER_ROOT_HASH,
        logs_bloom: Default::default(),
        blob_gas_used: Some(0),
        excess_blob_gas: Some(0),
        parent_beacon_block_root: Some(B256::ZERO),
        requests_hash: None,
    };

    let mut buf = Vec::new();
    header.encode(&mut buf);
    println!("Encoded length: {}", buf.len());
    println!("Encoded hex: {}", hex::encode(&buf));

    let mut slice = &buf[..];
    let decoded = Header::decode(&mut slice).expect("Failed to decode header");

    assert_eq!(header, decoded);
}

#[test]
fn test_header_rlp_prague() {
    let header = Header {
        number: 0,
        timestamp: 450,
        gas_limit: 30_000_000,
        state_root: B256::ZERO,
        beneficiary: Address::ZERO,
        difficulty: U256::ZERO,
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(1_000_000_000),
        extra_data: Bytes::from(vec![0x68, 0x69, 0x76, 0x65, 0x63, 0x68, 0x61, 0x69, 0x6e]),
        transactions_root: alloy_trie::EMPTY_ROOT_HASH,
        receipts_root: alloy_trie::EMPTY_ROOT_HASH,
        withdrawals_root: Some(alloy_trie::EMPTY_ROOT_HASH),
        gas_used: 0,
        parent_hash: B256::ZERO,
        ommers_hash: EMPTY_OMMER_ROOT_HASH,
        logs_bloom: Default::default(),
        blob_gas_used: Some(0),
        excess_blob_gas: Some(0),
        parent_beacon_block_root: Some(B256::ZERO),
        requests_hash: Some(alloy_trie::EMPTY_ROOT_HASH),
    };

    let mut buf = Vec::new();
    header.encode(&mut buf);
    println!("Encoded length: {}", buf.len());

    let mut slice = &buf[..];
    let decoded = Header::decode(&mut slice).expect("Failed to decode header");

    assert_eq!(header, decoded);
}
