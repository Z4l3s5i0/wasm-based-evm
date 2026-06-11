use tempfile::NamedTempFile;
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_storage::write_traits::*;
use wasix_eth_types::*;
use alloy_primitives::{Address, B256, U256, Bytes};

fn setup_db() -> EthDatabase {
    let tmp_file = NamedTempFile::new().unwrap();
    EthDatabase::open(tmp_file.path()).unwrap()
}

#[test]
fn test_batch_writer_atomicity() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let addr = Address::repeat_byte(0xaa);
    let key = "batch_test".to_string();
    let val = Bytes::from(vec![1]);

    // Start a batch
    let batch = writer.begin_batch().unwrap();
    batch.update_account(addr, TrieAccount::default()).unwrap();
    batch.set_metadata(key.clone(), val.clone()).unwrap();
    
    // Reader shouldn't see changes before commit
    assert!(reader.account(addr, None).unwrap().is_none());
    assert!(reader.get_metadata(key.clone()).unwrap().is_none());
    
    // Commit the batch
    batch.commit().unwrap();
    
    // Reader should see changes after commit
    assert!(reader.account(addr, None).unwrap().is_some());
    assert_eq!(reader.get_metadata(key).unwrap().unwrap(), val);
}

#[test]
fn test_batch_writer_rollback_on_drop() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let addr = Address::repeat_byte(0xbb);

    {
        let batch = writer.begin_batch().unwrap();
        batch.update_account(addr, TrieAccount::default()).unwrap();
        // batch is dropped here without commit
    }

    // Reader shouldn't see changes
    assert!(reader.account(addr, None).unwrap().is_none());
}

#[test]
fn test_batch_writer_multiple_ops() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let addr = Address::repeat_byte(0xcc);
    let hash = B256::repeat_byte(0xdd);
    let code = Bytes::from(vec![1, 2, 3]);

    let batch = writer.begin_batch().unwrap();
    batch.update_account(addr, TrieAccount::default()).unwrap();
    batch.insert_bytecode(hash, code.clone()).unwrap();
    batch.update_storage(addr, B256::ZERO, U256::from(1)).unwrap();
    batch.commit().unwrap();

    assert!(reader.account(addr, None).unwrap().is_some());
    assert_eq!(reader.bytecode(hash).unwrap().unwrap(), code);
    assert_eq!(reader.storage(addr, B256::ZERO, None).unwrap(), U256::from(1));
}
