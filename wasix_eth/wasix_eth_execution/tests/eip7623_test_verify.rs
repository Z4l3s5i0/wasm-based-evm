use wasix_eth_execution::config::calculate_intrinsic_gas;
use wasix_eth_types::Transaction;
use wasix_eth_types::Hardfork;
use alloy_primitives::Bytes;

#[test]
fn test_eip7623_intrinsic_gas_discrepancy() {
    // Create a data-heavy transaction
    // 1000 bytes of non-zero data
    let data = vec![1u8; 1000];
    let tx = create_mock_tx(data);

    // Current implementation uses: 21000 + 1000 * 48 = 69000
    // Expected EIP-7623: max(21000 + 1000 * 16, 21000 + 1000 * 40) = max(37000, 61000) = 61000
    
    let intrinsic_gas = calculate_intrinsic_gas(&tx, Hardfork::Prague);
    
    println!("Intrinsic gas for 1000 non-zero bytes: {}", intrinsic_gas);
    
    // This is expected to fail with the current implementation (it will be 69000)
    assert_eq!(intrinsic_gas, 61000, "Intrinsic gas should follow EIP-7623 spec (max(base + data_cost, base + floor_cost))");
}

#[test]
fn test_eip7623_zero_bytes() {
    // 1000 bytes of zero data
    let data = vec![0u8; 1000];
    let tx = create_mock_tx(data);

    // Current implementation uses: 21000 + 1000 * 48 = 69000
    // Expected EIP-7623: max(21000 + 1000 * 4, 21000 + 1000 * 10) = max(25000, 31000) = 31000
    
    let intrinsic_gas = calculate_intrinsic_gas(&tx, Hardfork::Prague);
    
    println!("Intrinsic gas for 1000 zero bytes: {}", intrinsic_gas);
    
    // This is expected to fail with the current implementation (it will be 69000)
    assert_eq!(intrinsic_gas, 31000, "Intrinsic gas for zero bytes should follow EIP-7623 spec");
}

fn create_mock_tx(data: Vec<u8>) -> Transaction {
    let tx_legacy = wasix_eth_types::TxLegacy {
        chain_id: Some(1),
        nonce: 0,
        gas_price: 1,
        gas_limit: 100000,
        to: wasix_eth_types::TxKind::Call(alloy_primitives::Address::ZERO),
        value: alloy_primitives::U256::ZERO,
        input: Bytes::from(data),
    };
    
    Transaction::Legacy(wasix_eth_types::Signed::new_unchecked(tx_legacy, alloy_primitives::Signature::test_signature(), alloy_primitives::B256::ZERO))
}
