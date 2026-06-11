use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::AccountProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{AccountWriter, BytecodeWriter, MetadataWriter};
use wasix_eth_types::*;
use tempfile::TempDir;

#[tokio::test]
async fn test_storage_deletion_root() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_storage_deletion.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());
    write_provider.set_metadata("chain_id".to_string(), 1u32.to_be_bytes().to_vec().into()).expect("chain id set");

    // 1. Deploy a contract that does SSTORE
    // PUSH1 100, PUSH1 1, SSTORE -> sets slot 1 to 100
    let code = Bytes::from(vec![0x60, 0x64, 0x60, 0x01, 0x55]);
    let contract_address = Address::repeat_byte(0xCC);
    let code_hash = keccak256(&code);
    
    write_provider.update_account(contract_address, TrieAccount {
        nonce: 1,
        balance: U256::ZERO,
        storage_root: EMPTY_ROOT_HASH,
        code_hash,
    }).unwrap();
    write_provider.insert_bytecode(code_hash, code).unwrap();

    let execution = EthExecutionProvider::new(read_provider.clone(), write_provider.clone());

    let tx1 = Transaction::Legacy(Signed::new_unchecked(
        TxLegacy {
            nonce: 0,
            gas_price: 1,
            gas_limit: 100000,
            to: contract_address.into(),
            value: U256::ZERO,
            input: Bytes::default(),
            chain_id: Some(1),
        },
        Signature::test_signature(),
        B256::default(),
    ));
    let sender = tx1.recover_signer().unwrap();
    write_provider.update_account(sender, TrieAccount {
        nonce: 0,
        balance: U256::from(1000000000000000000u64), // More funds
        storage_root: EMPTY_ROOT_HASH,
        code_hash: B256::default(),
    }).unwrap();

    let mut block1 = Block::<Transaction>::default();
    block1.header.number = 1;
    block1.body.transactions.push(tx1);

    let parent_hash = block1.header.hash_slow();
    execution.execute_block(block1).expect("Execution of block 1 failed");

    let account_after_b1 = read_provider.account(contract_address, None).unwrap().unwrap();
    let root_after_b1 = account_after_b1.storage_root;
    assert_ne!(root_after_b1, EMPTY_ROOT_HASH);

    // Block 2: Set slot 1 to 0 (delete)
    // PUSH1 0, PUSH1 1, SSTORE
    let code2 = Bytes::from(vec![0x60, 0x00, 0x60, 0x01, 0x55]);
    let code_hash2 = keccak256(&code2);
    // Replace contract code for next tx
    write_provider.insert_bytecode(code_hash2, code2).unwrap();
    let mut contract_acc = read_provider.account(contract_address, None).unwrap().unwrap();
    contract_acc.code_hash = code_hash2;
    write_provider.update_account(contract_address, contract_acc).unwrap();

    let tx2_inner = TxLegacy {
        nonce: 1,
        gas_price: 1,
        gas_limit: 100000,
        to: contract_address.into(),
        value: U256::ZERO,
        input: Bytes::default(),
        chain_id: Some(1),
    };
    let tx2 = Transaction::Legacy(Signed::new_unchecked(
        tx2_inner,
        Signature::test_signature(),
        B256::default(),
    ));

    let sender2 = tx2.recover_signer().unwrap();
    write_provider.update_account(sender2, TrieAccount {
        nonce: 1, // Start with nonce 1 in this block (Wait, no, it was 0 in block 1, incremented to 1)
        balance: U256::from(1000000000000000000u64),
        storage_root: EMPTY_ROOT_HASH,
        code_hash: B256::default(),
    }).unwrap();

    let mut block2 = Block::<Transaction>::default();
    block2.header.number = 2;
    block2.header.parent_hash = parent_hash;
    block2.body.transactions.push(tx2);

    execution.execute_block(block2).expect("Execution of block 2 failed");

    let account_after_b2 = read_provider.account(contract_address, None).unwrap().unwrap();
    assert_eq!(account_after_b2.storage_root, EMPTY_ROOT_HASH, "Storage root should be empty after deleting only slot");

    // Block 3: Storage Reset
    // We can't easily trigger storage_reset from simple EVM code (it's usually contract destruction),
    // but we can manually test the apply_changeset logic if we want.
    // However, our SSTORE(0) fix already covers the common case.
}




#[tokio::test]
async fn test_state_root_mismatch_fails() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_mismatch.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());
    write_provider.set_metadata("chain_id".to_string(), 1u32.to_be_bytes().to_vec().into()).expect("chain id set");

    let execution = EthExecutionProvider::new(read_provider, write_provider);

    let mut block = Block::<Transaction>::default();
    block.header.number = 1;
    // Set an obviously wrong state root
    block.header.state_root = B256::repeat_byte(0xee);

    let result = execution.execute_block(block);

    assert!(result.is_err(), "Execution should fail when state root mismatches");
    let err_msg = result.err().unwrap().to_string();
    assert!(err_msg.contains("State root mismatch"), "Error message should mention state root mismatch: {}", err_msg);
}