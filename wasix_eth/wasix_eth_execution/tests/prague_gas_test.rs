use wasix_eth_execution::config::calculate_intrinsic_gas;
use wasix_eth_types::{Transaction, Hardfork, TxEip7702, Signed, B256, Address, U256, Bytes, Signature};
use alloy_eips::eip7702::{Authorization, SignedAuthorization};

#[test]
fn test_eip7702_intrinsic_gas() {
    // EIP-7702 intrinsic gas: 21000 + 16 * non-zero calldata + 2500 * auth_list_len
    
    // Case 1: 0 data, 1 authorization
    let tx1 = create_eip7702_tx(vec![], 1);
    let gas1 = calculate_intrinsic_gas(&tx1, Hardfork::Prague);
    println!("EIP-7702 (0 data, 1 auth) gas: {}", gas1);
    // Expected: 21000 + 0 + 2500 = 23500
    // If it hit EIP-7623 floor: max(23500, 21000 + 0) = 23500
    assert_eq!(gas1, 23500);

    // Case 2: 0 data, 2 authorizations
    let tx2 = create_eip7702_tx(vec![], 2);
    let gas2 = calculate_intrinsic_gas(&tx2, Hardfork::Prague);
    println!("EIP-7702 (0 data, 2 auth) gas: {}", gas2);
    // Expected: 21000 + 0 + 5000 = 26000
    assert_eq!(gas2, 26000);
}

#[test]
fn test_eip7623_floor_gas() {
    // EIP-7623: floor = 21000 + tokens * 10
    // Tokens = 4 per non-zero byte, 1 per zero byte
    
    // Legacy TX, 100 bytes non-zero data
    // Intrinsic: 21000 + 100 * 16 = 22600
    // Tokens: 100 * 4 = 400
    // Floor: 21000 + 400 * 10 = 25000
    // Since 22600 < 25000, should be 25000
    
    let data = vec![1u8; 100];
    let tx = create_legacy_tx(data);
    let gas = calculate_intrinsic_gas(&tx, Hardfork::Prague);
    println!("Legacy (100 non-zero bytes) gas: {}", gas);
    assert_eq!(gas, 25000);
}

fn create_eip7702_tx(data: Vec<u8>, auth_count: usize) -> Transaction {
    let mut auth_list = Vec::new();
    for _ in 0..auth_count {
        let auth = Authorization {
            chain_id: U256::from(1),
            address: Address::ZERO,
            nonce: 0,
        };
        auth_list.push(SignedAuthorization::new_unchecked(
            auth,
            0,
            U256::ZERO,
            U256::ZERO,
        ));
    }
    
    let tx = TxEip7702 {
        chain_id: 1,
        nonce: 0,
        max_priority_fee_per_gas: 1,
        max_fee_per_gas: 1,
        gas_limit: 100000,
        to: Address::ZERO,
        value: U256::ZERO,
        input: Bytes::from(data),
        access_list: Default::default(),
        authorization_list: auth_list,
    };
    
    Transaction::Eip7702(Signed::new_unchecked(tx, Signature::test_signature(), B256::ZERO))
}

fn create_legacy_tx(data: Vec<u8>) -> Transaction {
    let tx = wasix_eth_types::TxLegacy {
        chain_id: Some(1),
        nonce: 0,
        gas_price: 1,
        gas_limit: 100000,
        to: wasix_eth_types::TxKind::Call(Address::ZERO),
        value: U256::ZERO,
        input: Bytes::from(data),
    };
    
    Transaction::Legacy(Signed::new_unchecked(tx, Signature::test_signature(), B256::ZERO))
}
