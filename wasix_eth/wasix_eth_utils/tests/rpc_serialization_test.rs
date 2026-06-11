use wasix_eth_types::{Transaction, Header, B256, U256, Address, Bloom, Block, BlockBody};
use wasix_eth_utils::block_mapper::BlockMapper;
use wasix_eth_utils::transaction_mapper::TransactionMapper;

#[test]
fn test_rpc_block_serialization() {
    let header = Header {
        parent_hash: B256::ZERO,
        ommers_hash: B256::ZERO,
        beneficiary: Address::ZERO,
        state_root: B256::ZERO,
        transactions_root: B256::ZERO,
        receipts_root: B256::ZERO,
        logs_bloom: Bloom::ZERO,
        difficulty: U256::from(0x20000),
        number: 1,
        gas_limit: 0x23f3e20,
        gas_used: 0x527eb,
        timestamp: 10,
        extra_data: vec![].into(),
        mix_hash: B256::ZERO,
        nonce: [0u8; 8].into(),
        base_fee_per_gas: None,
        withdrawals_root: None,
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        requests_hash: None,
    };

    let block = Block {
        header: header.clone(),
        body: BlockBody {
            transactions: vec![],
            ommers: vec![],
            withdrawals: None,
        },
    };

    let chain_config = wasix_eth_types::ChainConfig::default();
    let td = Some(U256::from(0x40000));
    
    let rpc_block = BlockMapper::to_rpc_block(block.clone(), true, &chain_config, td);
    let json = serde_json::to_string(&rpc_block).unwrap();
    // println!("{}", json);

    // Verify fields
    assert!(json.contains("\"logsBloom\""));
    assert!(json.contains("\"totalDifficulty\""));
    
    // Check if totalDifficulty is omitted when we force Paris fork
    let mut paris_config = wasix_eth_types::ChainConfig::default();
    paris_config.merge_netsplit_block = Some(0);
    let rpc_block_paris = BlockMapper::to_rpc_block(Block {
        header: header.clone(),
        body: BlockBody {
            transactions: vec![],
            ommers: vec![],
            withdrawals: None,
        },
    }, true, &paris_config, td);
    let json_paris = serde_json::to_string(&rpc_block_paris).unwrap();
    assert!(!json_paris.contains("\"totalDifficulty\""), "totalDifficulty should be omitted in Paris fork");
    
    let _signer = Address::from([0x74; 20]);
    let tx = Transaction::Legacy(wasix_eth_types::Signed::new_unchecked(
        wasix_eth_types::TxLegacy {
            chain_id: None,
            nonce: 0,
            gas_price: 1,
            gas_limit: 21000,
            to: wasix_eth_types::TxKind::Call(Address::ZERO),
            value: U256::ZERO,
            input: vec![].into(),
        },
        wasix_eth_types::Signature::test_signature(),
        B256::ZERO,
    ));

    let rpc_tx = TransactionMapper::to_rpc_transaction(tx, Some((1, B256::ZERO, 0)), Some(header.clone()));
    let json_tx = serde_json::to_string(&rpc_tx).unwrap();
    println!("TX JSON: {}", json_tx);
    assert!(!json_tx.contains("\"signer\""), "Transaction JSON should not contain 'signer'");

    let rpc_block = BlockMapper::to_rpc_block(block, true, &chain_config, td);
    let json_block = serde_json::to_string(&rpc_block).unwrap();
    println!("BLOCK JSON: {}", json_block);
    assert!(json_block.contains("\"logsBloom\""), "Block JSON must contain 'logsBloom'");
    
    // Check for duplication of logsBloom (if it appears twice, there will be two occurrences of the string)
    let count = json_block.matches("\"logsBloom\"").count();
    assert_eq!(count, 1, "logsBloom should only appear once in JSON, found {}", count);
}

#[test]
fn test_london_fork_base_fee() {
    let mut london_config = wasix_eth_types::ChainConfig::default();
    london_config.london_block = Some(10);
    
    let header = Header {
        number: 11,
        timestamp: 100,
        base_fee_per_gas: Some(1000000000), // 1 Gwei
        ..Default::default()
    };
    
    let block = Block {
        header: header.clone(),
        body: BlockBody {
            transactions: vec![],
            ommers: vec![],
            withdrawals: None,
        },
    };
    
    let rpc_block = BlockMapper::to_rpc_block(block, false, &london_config, Some(U256::from(0x381d40)));
    let json = serde_json::to_string(&rpc_block).unwrap();
    println!("London Block JSON: {}", json);
    
    assert!(json.contains("\"baseFeePerGas\":\"0x3b9aca00\""), "Should contain baseFeePerGas in hex");
    // The test in the issue says totalDifficulty should be omitted at block 0x1b (27).
    // Wait, let's check the test expectation in the issue again.
    // It says:
    // -- "totalDifficulty": "0x381d40",
    // This confirms it should be omitted.
    assert!(!json.contains("\"totalDifficulty\""), "totalDifficulty should be omitted in London fork (according to the failing test)");
}
