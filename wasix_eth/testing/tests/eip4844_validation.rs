use wasix_eth_storage::EthDatabase;
use wasix_eth_types::*;
use wasix_eth_types::genesis::{GenesisConfiguration, GenesisAccount};
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use alloy_primitives::{address, b256, uint, B256, U256, Bytes, keccak256, Signature};
use alloy_consensus::{TxEip4844, Signed};
use tempfile::TempDir;
use wasix_eth_execution::execution_provider::ExecutionProvider;
use wasix_eth_storage::read_traits::{AccountProvider, BlockProvider, HeaderProvider};

#[tokio::test]
async fn test_eip4844_blob_fee_deduction() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_eip4844.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let beneficiary = address!("0000000000000000000000000000000000000002");
    
    // The test signature recovers to this address
    let sender = address!("04d3e9844cf986136638550bd3d13e624b40f18b");

    let mut genesis_config = GenesisConfiguration::default();
    genesis_config.config.chain_id = 1;
    // Activate Cancun at block 0
    genesis_config.config.cancun_time = Some(0);
    
    let initial_balance = uint!(10000000000000000000_U256); // 10 ETH
    genesis_config.alloc.insert(sender, GenesisAccount {
        balance: initial_balance,
        ..Default::default()
    });

    let read_provider = DatabaseReadProvider::new(db.inner());
    db.init_genesis(genesis_config.clone()).unwrap();

    let genesis_header = read_provider.header(BlockId::Number(BlockNumberOrTag::Number(0))).unwrap().unwrap();
    let genesis_state_root = genesis_header.state_root;
    
    println!("Genesis state root: {:?}", genesis_state_root);
    let sender_acc_gen = read_provider.account(sender, Some(genesis_state_root)).unwrap().unwrap();
    println!("Sender balance in genesis: {}", sender_acc_gen.balance);
    let header = Header {
        number: 1,
        parent_hash: read_provider.block_hash(0).unwrap().unwrap(),
        state_root: genesis_state_root,
        timestamp: 100,
        base_fee_per_gas: Some(10),
        excess_blob_gas: Some(0),
        blob_gas_used: Some(131072),
        ..Default::default()
    };

    let blob_hash = b256!("0100000000000000000000000000000000000000000000000000000000000000");
    
    let tx_inner = TxEip4844 {
        chain_id: 1,
        nonce: 0,
        max_priority_fee_per_gas: 1,
        max_fee_per_gas: 20,
        gas_limit: 21000,
        to: beneficiary,
        value: uint!(1000_U256),
        input: Bytes::new(),
        access_list: Default::default(),
        max_fee_per_blob_gas: 20,
        blob_versioned_hashes: vec![blob_hash],
    };
    
    let signature = Signature::test_signature();
    let signed_tx = Signed::new_unchecked(tx_inner.clone(), signature, tx_hash(tx_inner.clone()));
    let tx = Transaction::Eip4844(signed_tx.into());

    // Use the execution provider to execute the block
    let execution_provider = wasix_eth_execution::execution_provider::EthExecutionProvider::new(
        DatabaseReadProvider::new(db.inner()),
        DatabaseWriteProvider::new(db.inner()),
    );
    
    let block = Block {
        header: header.clone(),
        body: BlockBody {
            transactions: vec![tx.clone()],
            ommers: Vec::new(),
            withdrawals: None,
        },
    };

    let (_executed_block, receipts) = execution_provider.execute_block_with_commit(block, true).unwrap();
    
    assert_eq!(receipts.len(), 1);
    
    // Verify balance deduction
    let read_provider = DatabaseReadProvider::new(db.inner());
    let sender_acc = read_provider.account(sender, None).unwrap().unwrap();
    
    let gas_used = receipts[0].receipt.cumulative_gas_used;
    let gas_price = 11; // base_fee (10) + priority_fee (1)
    let execution_fee = U256::from(gas_used) * U256::from(gas_price);
    
    let blob_gas_used = 131072; // 1 blob
    let blob_base_fee = 1; // calculated from excess_blob_gas=0
    let blob_fee = U256::from(blob_gas_used) * U256::from(blob_base_fee);
    
    let expected_balance = initial_balance - execution_fee - blob_fee; // deducted value (no transfer on OutOfGas)
    
    println!("Initial balance: {}", initial_balance);
    println!("Execution fee: {}", execution_fee);
    println!("Blob fee: {}", blob_fee);
    println!("Value: 1000");
    println!("Expected balance: {}", expected_balance);
    println!("Actual balance:   {}", sender_acc.balance);
    
    assert_eq!(sender_acc.balance, expected_balance, "Balance mismatch after EIP-4844 transaction");
}

fn tx_hash<T: alloy_consensus::SignableTransaction<Signature>>(tx: T) -> B256 {
    let mut buf = Vec::new();
    tx.encode_for_signing(&mut buf);
    keccak256(&buf)
}
