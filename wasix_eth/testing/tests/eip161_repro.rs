use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::AccountProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{AccountWriter, BytecodeWriter};
use wasix_eth_types::*;
use wasix_eth_utils::info;
use tempfile::TempDir;

#[tokio::test]
async fn test_eip161_empty_account_state_clearing() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_eip161.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());
    let execution = EthExecutionProvider::new(read_provider.clone(), write_provider.clone());

    // Address that will be "touched" but remains empty
    let empty_address = Address::repeat_byte(0xEE);
    
    // 1. Send a transaction that touches empty_address
    // PUSH1 0 (value), PUSH20 empty_address, PUSH1 0 (gas), CALL
    // Opcode sequence: 60 00 60 00 60 00 60 00 60 00 73 <address> 60 00 f1
    let mut call_data = vec![0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x60, 0x00, 0x73];
    call_data.extend_from_slice(empty_address.as_slice());
    call_data.extend_from_slice(&[0x60, 0x00, 0xf1]);
    let code = Bytes::from(call_data);
    let contract_address = Address::repeat_byte(0xCC);
    let code_hash = keccak256(&code);
    
    write_provider.update_account(contract_address, TrieAccount {
        nonce: 1,
        balance: U256::from(100),
        storage_root: EMPTY_ROOT_HASH,
        code_hash,
    }).unwrap();
    write_provider.insert_bytecode(code_hash, code).unwrap();

    let tx = Transaction::Legacy(Signed::new_unchecked(
        TxLegacy {
            nonce: 0,
            gas_price: 1,
            gas_limit: 1000000,
            to: contract_address.into(),
            value: U256::ZERO,
            input: Bytes::default(),
            chain_id: Some(31133),
        },
        Signature::test_signature(),
        B256::default(),
    ));
    let sender = tx.recover_signer().unwrap();
    write_provider.update_account(sender, TrieAccount {
        nonce: 0,
        balance: U256::from(1000000000000000000u64),
        storage_root: EMPTY_ROOT_HASH,
        code_hash: B256::default(),
    }).unwrap();

    let mut block = Block::<Transaction>::default();
    block.header.number = 1;
    block.body.transactions.push(tx);

    // Use B256::ZERO to skip verification during first pass
    block.header.state_root = B256::ZERO;

    let (results, result_block) = execution.run_execution(block.body.transactions.clone(), block.clone(), false).expect("Execution (first pass) failed");
    block.header.state_root = result_block.header.state_root;
    block.header.transactions_root = result_block.header.transactions_root;
    block.header.receipts_root = result_block.header.receipts_root;

    // Second pass WITH apply=true to commit changes
    let (results, final_block) = execution.run_execution(block.body.transactions.clone(), block.clone(), true).expect("Execution failed");
    
    info!("Checking if account {:?} exists", empty_address);
    // EIP-161: The touched empty account should NOT exist in the state
    let account = read_provider.account(empty_address, None).unwrap();
    if let Some(ref acc) = account {
        info!("Account exists: {:?}", acc);
    } else {
        info!("Account does not exist");
    }
    assert!(account.is_none(), "Touched empty account should not exist in the state according to EIP-161");
}
