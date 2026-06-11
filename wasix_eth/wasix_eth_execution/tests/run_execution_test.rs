use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{AccountWriter, MetadataWriter};
use wasix_eth_types::*;
use tempfile::TempDir;
use wasix_eth_storage::read_traits::AccountProvider;

#[tokio::test]
async fn test_run_execution_simple_transfer() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_run.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());
    write_provider.set_metadata("chain_id".to_string(), 1u32.to_be_bytes().to_vec().into()).expect("chain id set");

    let tx = Transaction::Legacy(Signed::new_unchecked(
        TxLegacy {
            nonce: 0,
            gas_price: 1,
            gas_limit: 21000,
            to: Address::repeat_byte(0x20).into(),
            value: U256::from(100),
            input: Bytes::default(),
            chain_id: Some(1),
        },
        Signature::test_signature(),
        B256::default(),
    ));

    let sender = tx.recover_signer().unwrap();
    let recipient = tx.to().unwrap();

    write_provider.update_account(sender, TrieAccount {
        nonce: 0,
        balance: U256::from(1000000),
        storage_root: EMPTY_ROOT_HASH,
        code_hash: B256::default(),
    }).unwrap();

    let execution = EthExecutionProvider::new(read_provider.clone(), write_provider);

    let mut block = Block::<Transaction>::default();
    block.header.number = 1;
    block.header.timestamp = 1000;
    block.header.gas_limit = 1000000;
    block.header.state_root = B256::ZERO;

    // Run execution (apply_changes = false)
    let (results, final_block) = execution.run_execution(vec![tx.clone()], block.clone(), false).expect("run_execution failed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].gas_used, 21000);
    assert_eq!(final_block.body.transactions.len(), 1);

    // Verify state NOT changed in DB
    let sender_acc = read_provider.account(sender, None).unwrap().unwrap();
    assert_eq!(sender_acc.balance, U256::from(1000000));

    // Run execution (apply_changes = true)
    let (results2, _final_block2) = execution.run_execution(vec![tx.clone()], block.clone(), true).expect("run_execution apply failed");
    assert_eq!(results2.len(), 1);
    
    // Verify state CHANGED in DB
    let sender_acc_after = read_provider.account(sender, None).unwrap().unwrap();
    assert!(sender_acc_after.balance < U256::from(1000000));
    assert_eq!(read_provider.account(recipient, None).unwrap().unwrap().balance, U256::from(100));
}
