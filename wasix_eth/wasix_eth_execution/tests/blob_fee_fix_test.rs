use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::read_traits::BlockProvider;
use wasix_eth_types::*;
use tempfile::TempDir;
use alloy_primitives::{Address, B256, U256, Bytes, Signature};
use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_storage::EthDatabase;

#[tokio::test]
async fn test_double_blob_fee_deduction_fix() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_blob_fee.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let mut config = genesis::GenesisConfiguration::default();
    config.config.chain_id = 1;
    config.config.cancun_time = Some(0); 

    // Create an EIP-4844 transaction
    let tx_4844 = TxEip4844 {
        chain_id: 1,
        nonce: 0,
        max_priority_fee_per_gas: 1_000_000_000,
        max_fee_per_gas: 20_000_000_000,
        gas_limit: 21000,
        to: Address::repeat_byte(0x22).into(),
        value: U256::from(100),
        input: Bytes::new(),
        access_list: Default::default(),
        blob_versioned_hashes: vec![B256::repeat_byte(0xaa)],
        max_fee_per_blob_gas: 100_000_000_000, // High blob gas price
    };

    // Signature doesn't matter for mock execution if we use recovery override or just fake it
    let signature = Signature::test_signature();
    let tx = Transaction::Eip4844(Signed::new_unchecked(TxEip4844Variant::from(tx_4844), signature, B256::repeat_byte(0x55)));

    let sender = tx.recover_signer().expect("Failed to recover signer");
    let initial_balance = U256::from(5000000000000000000u128); // 5 ETH
    config.alloc.insert(sender, GenesisAccount {
        balance: initial_balance,
        ..Default::default()
    });

    db.init_genesis(config).unwrap();

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());
    let execution = EthExecutionProvider::new(read_provider.clone(), write_provider.clone());

    let mut block = Block::<Transaction>::default();
    block.header.number = 1;
    block.header.timestamp = 1000;
    block.header.parent_hash = read_provider.block_hash(0).unwrap().unwrap();
    block.header.blob_gas_used = Some(131072);
    block.header.excess_blob_gas = Some(0);
    block.header.base_fee_per_gas = Some(10_000_000_000);
    block.header.parent_beacon_block_root = Some(B256::ZERO);
    block.body.transactions.push(tx);

    // This should NOT fail with OutOfFund now
    let (final_block, _) = execution.execute_block(block).expect("Execution failed");
    
    use wasix_eth_storage::read_traits::AccountProvider;
    let account = read_provider.account(sender, Some(final_block.header.state_root)).unwrap().expect("Sender should exist");
    
    println!("Initial balance: {}", initial_balance);
    println!("Final balance:   {}", account.balance);
    
    assert!(account.balance < initial_balance);
}
