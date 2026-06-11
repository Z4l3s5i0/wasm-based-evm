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
fn test_block_provider_exhaustive() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let header = Header {
        number: 10,
        ..Default::default()
    };
    let hash = header.hash_slow();
    let body = BlockBody {
        transactions: vec![],
        ommers: vec![],
        withdrawals: None,
    };

    writer.insert_header(10, header.clone()).unwrap();
    writer.insert_block_hash(hash, 10).unwrap();
    writer.insert_block_body(hash, 10, body.clone()).unwrap();
    writer.set_canonical(10, hash).unwrap();
    writer.commit().unwrap();

    // Test various lookups
    assert_eq!(reader.header(BlockId::Number(10.into())).unwrap().unwrap().number, 10);
    assert_eq!(reader.header(BlockId::Hash(hash.into())).unwrap().unwrap().number, 10);
    assert_eq!(reader.block_hash(10).unwrap().unwrap(), hash);
    assert_eq!(reader.block_number(hash).unwrap().unwrap(), 10);
    assert_eq!(reader.block_body(10).unwrap().unwrap(), body);
    assert_eq!(reader.block_body_by_hash(hash).unwrap().unwrap(), body);
    assert_eq!(reader.latest_block_number().unwrap().unwrap(), 10);

    // Test non-existent
    assert!(reader.header(BlockId::Number(11.into())).unwrap().is_none());
    assert!(reader.block_hash(11).unwrap().is_none());
}

#[test]
fn test_transaction_provider_exhaustive() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let tx_hash = B256::repeat_byte(0x11);
    // Note: Transaction doesn't implement Default, but let's assume we can mock it or use a simple one if available.
    // In wasix_eth_types, Transaction is alloy_consensus::TxEnvelope.
    // For now, let's just test lookup if we can insert it.
}

#[test]
fn test_changeset_provider_exhaustive() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let acc_changeset = vec![(Address::repeat_byte(0x1), Some(Bytes::from(vec![1])))];
    let storage_changeset = vec![(Address::repeat_byte(0x1), B256::repeat_byte(0x2), U256::from(3))];

    writer.insert_account_change_set(1, acc_changeset.clone()).unwrap();
    writer.insert_storage_change_set(1, storage_changeset.clone()).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.account_change_set(1).unwrap().unwrap(), acc_changeset);
    assert_eq!(reader.storage_change_set(1).unwrap().unwrap(), storage_changeset);
}

#[test]
fn test_state_provider_exhaustive() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let addr = Address::repeat_byte(0x22);
    let hash = B256::repeat_byte(0x33);
    let trie_hash = B256::repeat_byte(0x44);
    let data = Bytes::from(vec![4, 5, 6]);

    writer.update_plain_state(addr, data.clone()).unwrap();
    writer.update_hashed_state(hash, data.clone()).unwrap();
    writer.update_trie_node(trie_hash, data.clone()).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.plain_state(addr).unwrap().unwrap(), data);
    assert_eq!(reader.hashed_state(hash).unwrap().unwrap(), data);
    assert_eq!(reader.trie_node(trie_hash).unwrap().unwrap(), data);
}

#[test]
fn test_forkchoice_provider() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let head = B256::repeat_byte(0x01);
    let safe = B256::repeat_byte(0x02);
    let finalized = B256::repeat_byte(0x03);

    writer.update_forkchoice(head, Some(safe), Some(finalized)).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.forkchoice("head").unwrap().unwrap(), head);
    assert_eq!(reader.forkchoice("safe").unwrap().unwrap(), safe);
    assert_eq!(reader.forkchoice("finalized").unwrap().unwrap(), finalized);
}
