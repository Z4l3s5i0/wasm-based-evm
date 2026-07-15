use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_storage::write_traits::*;
use wasix_eth_types::*;
use tempfile::tempdir;
use alloy_primitives::{B256, Address, U256, Bytes};

fn setup_db() -> EthDatabase {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("test.db")).unwrap();
    db.init_tables().unwrap();
    db
}

// --- DatabaseWriteProvider ---

#[test]
fn test_writer_new_success() {
    let db = setup_db();
    let _writer = DatabaseWriteProvider::new(db.inner());
}

#[test]
fn test_writer_commit_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.set_metadata("k".into(), vec![1].into()).unwrap();
    writer.commit().unwrap();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.get_metadata("k".into()).unwrap(), Some(vec![1].into()));
}

#[test]
fn test_writer_commit_edge_empty() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.commit().unwrap();
}

#[test]
fn test_writer_begin_batch_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let _batch = writer.begin_batch().unwrap();
}

// --- BatchWriter ---

#[test]
fn test_batch_commit_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.set_metadata("k".into(), vec![1].into()).unwrap();
    batch.commit().unwrap();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.get_metadata("k".into()).unwrap(), Some(vec![1].into()));
}

#[test]
fn test_batch_rollback_edge_drop() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.set_metadata("k".into(), vec![1].into()).unwrap();
    drop(batch);
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.get_metadata("k".into()).unwrap(), None);
}

// --- HeaderWriter (BatchWriter) ---

#[test]
fn test_batch_insert_header_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.insert_header(B256::repeat_byte(0x1), Header::default()).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_insert_header_edge_overwrite() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.insert_header(B256::repeat_byte(0x1), Header { number: 1, ..Default::default() }).unwrap();
    batch.insert_header(B256::repeat_byte(0x1), Header { number: 2, ..Default::default() }).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_insert_block_hash_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.insert_block_hash(B256::repeat_byte(0x1), 1).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.block_number(B256::repeat_byte(0x1)).unwrap(), Some(1));
}

// --- BlockWriter (BatchWriter) ---

#[test]
fn test_batch_set_canonical_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.set_canonical(1, B256::repeat_byte(0x1)).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_remove_canonical_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.set_canonical(1, B256::repeat_byte(0x1)).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    batch.remove_canonical(1).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.block_hash(1).unwrap().is_none());
}

#[test]
fn test_batch_insert_block_body_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let body = BlockBody { transactions: vec![], ommers: vec![], withdrawals: None };
    let hash = B256::repeat_byte(0x1);
    batch.insert_block_body(hash, 1, body).unwrap();
    batch.set_canonical(1, hash).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.block_body(1).unwrap().is_some());
}

#[test]
fn test_batch_update_forkchoice_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let h = B256::repeat_byte(0x1);
    batch.update_forkchoice(h, Some(h), Some(h)).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_add_payload_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let id = PayloadId::new([1; 8]);
    let block = Block::default();
    batch.add_payload(id, block, vec![], vec![], BlobsBundleV1::default()).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.get_payload(&id).is_some());
}

// --- TransactionWriter (BatchWriter) ---

#[test]
fn test_batch_insert_transaction_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let tx_inner = TxLegacy::default();
    let tx = Transaction::Legacy(Signed::new_unhashed(tx_inner, Signature::test_signature()));
    batch.insert_transaction(*tx.hash(), tx).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_insert_receipt_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let receipt = Receipt::default();
    let block_hash = B256::repeat_byte(0x1);
    batch.insert_receipt(block_hash, 0, receipt).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_insert_transaction_lookup_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let hash = B256::repeat_byte(0x1);
    let block_hash = B256::repeat_byte(0x2);
    batch.insert_block_hash(block_hash, 1).unwrap();
    batch.set_canonical(1, block_hash).unwrap();
    batch.insert_transaction_lookup(hash, block_hash, 0).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.transaction_block(hash).unwrap(), Some(1));
}

// --- AccountWriter (BatchWriter) ---

#[test]
fn test_batch_update_account_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.update_account(Address::repeat_byte(0x1), TrieAccount::default()).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_account_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_account(addr, TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    let acc = batch.account(addr, None).unwrap();
    assert!(acc.is_some());
}

#[test]
fn test_batch_account_failure_not_found() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let acc = batch.account(Address::repeat_byte(0x1), None).unwrap();
    assert!(acc.is_none());
}

#[test]
fn test_batch_accounts_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_account(addr, TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    let accs = batch.accounts().unwrap();
    assert_eq!(accs.len(), 1);
}

#[test]
fn test_batch_addresses_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_account(addr, TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    let addrs = batch.addresses().unwrap();
    assert_eq!(addrs.len(), 1);
    assert_eq!(addrs[0], addr);
}

#[test]
fn test_batch_transaction_count_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_account(addr, TrieAccount { nonce: 10, ..Default::default() }).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    let nonce = batch.transaction_count(addr, BlockId::latest(), None).unwrap();
    assert_eq!(nonce, 10);
}

#[test]
fn test_batch_transaction_count_edge_not_found() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let nonce = batch.transaction_count(Address::repeat_byte(0x1), BlockId::latest(), None).unwrap();
    assert_eq!(nonce, 0);
}

#[test]
fn test_batch_calculate_state_root_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let root = batch.calculate_state_root(false, None).unwrap();
    assert_ne!(root, B256::ZERO);
}

#[test]
fn test_batch_calculate_state_root_edge_with_data() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.update_account(addr, TrieAccount { nonce: 1, ..Default::default() }).unwrap();
    
    let root = batch.calculate_state_root(false, None).unwrap();
    assert_ne!(root, B256::ZERO);
    assert_ne!(root, EMPTY_ROOT_HASH);
}

#[test]
fn test_batch_calculate_storage_root_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let root = batch.calculate_storage_root(Address::repeat_byte(0x1), None).unwrap();
    assert_ne!(root, B256::ZERO);
}

#[test]
fn test_batch_calculate_storage_root_edge_with_data() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.update_storage(addr, B256::repeat_byte(0x1), U256::from(1)).unwrap();
    
    let root = batch.calculate_storage_root(addr, None).unwrap();
    assert_ne!(root, B256::ZERO);
    assert_ne!(root, EMPTY_ROOT_HASH);
}

#[test]
fn test_batch_remove_account_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_account(addr, TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    batch.remove_account(addr).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.account(addr, None).unwrap().is_none());
}

// --- StorageWriter (BatchWriter) ---

#[test]
fn test_batch_update_storage_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.update_storage(Address::repeat_byte(0x1), B256::repeat_byte(0x2), U256::from(1)).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_storage_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let slot = B256::repeat_byte(0x2);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_storage(addr, slot, U256::from(123)).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    let val = batch.storage(addr, slot, None).unwrap();
    assert_eq!(val, U256::from(123));
}

#[test]
fn test_batch_storage_failure_not_found() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let val = batch.storage(Address::repeat_byte(0x1), B256::repeat_byte(0x2), None).unwrap();
    assert_eq!(val, U256::ZERO);
}

#[test]
fn test_batch_account_storages_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_storage(addr, B256::repeat_byte(0x1), U256::from(1)).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    let storages = batch.account_storages(addr, None).unwrap();
    assert_eq!(storages.len(), 1);
}

#[test]
fn test_multi_state_coexistence() {
    let db = setup_db();
    let addr_a = Address::repeat_byte(0xA);
    let addr_b = Address::repeat_byte(0xB);

    // Execute Block A1 at height 1
    let writer_a = DatabaseWriteProvider::new(db.inner());
    let batch_a = writer_a.begin_batch().unwrap();
    batch_a.update_account(addr_a, TrieAccount { nonce: 1, balance: U256::from(100), ..Default::default() }).unwrap();
    let root_a1 = batch_a.calculate_state_root(false, None).unwrap();
    println!("Root A1: {:?}", root_a1);
    batch_a.commit().unwrap();

    // Execute Block B1 at height 1 (fork) with the same parent
    let writer_b = DatabaseWriteProvider::new(db.inner());
    let batch_b = writer_b.begin_batch().unwrap();
    // Re-add A if we want a real fork from same parent, but here we just want to test historical queries
    batch_b.update_account(addr_b, TrieAccount { nonce: 1, balance: U256::from(200), ..Default::default() }).unwrap();
    let root_b1 = batch_b.calculate_state_root(false, None).unwrap();
    println!("Root B1: {:?}", root_b1);
    batch_b.commit().unwrap();

    assert_ne!(root_a1, root_b1);

    // Verify querying state with root_A1 returns A1's data
    let reader = DatabaseReadProvider::new(db.inner());
    let acc_a = reader.account(addr_a, Some(root_a1)).unwrap().expect("Account A should exist in root A1");
    assert_eq!(acc_a.balance, U256::from(100));

    // Verify querying state with root_B1 returns B1's data
    let acc_b = reader.account(addr_b, Some(root_b1)).unwrap().expect("Account B should exist in root B1");
    assert_eq!(acc_b.balance, U256::from(200));
}

// --- BytecodeWriter (BatchWriter) ---

#[test]
fn test_batch_insert_bytecode_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.insert_bytecode(B256::repeat_byte(0x1), vec![1].into()).unwrap();
    batch.commit().unwrap();
}

// --- StateWriter (BatchWriter) ---

#[test]
fn test_batch_update_plain_state_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.update_plain_state(Address::repeat_byte(0x1), vec![1].into()).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_remove_plain_state_success() {
    let db = setup_db();
    let addr = Address::repeat_byte(0x1);
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_plain_state(addr, vec![1].into()).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    batch.remove_plain_state(addr).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.plain_state(addr).unwrap().is_none());
}

#[test]
fn test_batch_update_hashed_state_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let hash = B256::repeat_byte(0x1);
    batch.update_hashed_state(hash, vec![1].into()).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.hashed_state(hash).unwrap().is_some());
}

#[test]
fn test_batch_update_trie_node_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    let hash = B256::repeat_byte(0x1);
    batch.update_trie_node(hash, vec![2].into()).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.trie_node(B256::repeat_byte(0x1)).unwrap().is_some());
}

#[test]
fn test_batch_update_trie_node_edge_empty_hash() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.update_trie_node(B256::ZERO, vec![1].into()).unwrap();
    batch.commit().unwrap();
}

// --- ChangeSetWriter (BatchWriter) ---

#[test]
fn test_batch_insert_account_change_set_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.insert_account_change_set(1, vec![(Address::repeat_byte(0x1), None)]).unwrap();
    batch.commit().unwrap();
}

#[test]
fn test_batch_insert_storage_change_set_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.insert_storage_change_set(1, vec![(Address::repeat_byte(0x1), B256::repeat_byte(0x2), U256::from(1))]).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.storage_change_set(1).unwrap().is_some());
}

#[test]
fn test_batch_remove_change_set_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_account_change_set(1, vec![(Address::repeat_byte(0x1), None)]).unwrap();
    writer.insert_storage_change_set(1, vec![(Address::repeat_byte(0x1), B256::repeat_byte(0x2), U256::from(1))]).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    let batch = writer2.begin_batch().unwrap();
    batch.remove_change_set(1).unwrap();
    batch.commit().unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.account_change_set(1).unwrap().is_none());
    assert!(reader.storage_change_set(1).unwrap().is_none());
}

#[test]
fn test_batch_remove_change_set_edge_not_found() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.remove_change_set(1).unwrap();
    batch.commit().unwrap();
}

// --- MetadataWriter (BatchWriter) ---

#[test]
fn test_batch_set_metadata_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let batch = writer.begin_batch().unwrap();
    batch.set_metadata("k".into(), vec![1].into()).unwrap();
    batch.commit().unwrap();
}

// --- DatabaseWriteProvider Trait Impls (Forwarding) ---

#[test]
fn test_writer_insert_header_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_header(B256::repeat_byte(0x1), Header::default()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_insert_block_hash_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_block_hash(B256::repeat_byte(0x1), 1).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_set_canonical_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.set_canonical(1, B256::repeat_byte(0x1)).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_remove_canonical_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.set_canonical(1, B256::repeat_byte(0x1)).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_canonical(1).unwrap();
    writer2.commit().unwrap();
}

#[test]
fn test_writer_insert_block_body_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_block_body(B256::repeat_byte(0x1), 1, BlockBody::default()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_update_forkchoice_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_forkchoice(B256::repeat_byte(0x1), None, None).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_add_payload_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.add_payload(PayloadId::new([1; 8]), Block::default(), vec![], vec![], BlobsBundleV1::default()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_insert_transaction_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let tx_inner = TxLegacy::default();
    let tx = Transaction::Legacy(Signed::new_unhashed(tx_inner, Signature::test_signature()));
    writer.insert_transaction(*tx.hash(), tx).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_insert_receipt_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_receipt(B256::repeat_byte(0x1), 0, Receipt::default()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_insert_transaction_lookup_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let hash = B256::repeat_byte(0x1);
    let block_hash = B256::repeat_byte(0x2);
    writer.insert_transaction_lookup(hash, block_hash, 0).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_update_account_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_account(Address::repeat_byte(0x1), TrieAccount::default()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_remove_account_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_account(Address::repeat_byte(0x1), TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_account(Address::repeat_byte(0x1)).unwrap();
    writer2.commit().unwrap();
}

#[test]
fn test_writer_update_storage_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_storage(Address::repeat_byte(0x1), B256::repeat_byte(0x2), U256::from(1)).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_insert_bytecode_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_bytecode(B256::repeat_byte(0x1), vec![1].into()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_update_plain_state_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_plain_state(Address::repeat_byte(0x1), vec![1].into()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_remove_plain_state_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_plain_state(Address::repeat_byte(0x1), vec![1].into()).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_plain_state(Address::repeat_byte(0x1)).unwrap();
    writer2.commit().unwrap();
}

#[test]
fn test_writer_update_hashed_state_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_hashed_state(B256::repeat_byte(0x1), vec![1].into()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_update_trie_node_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.update_trie_node(B256::repeat_byte(0x1), vec![1].into()).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_register_peer_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let peer = PeerEntry { peer_id: "p1".into(), discovery_addr: "127.0.0.1:1".parse().unwrap(), p2p_addr: "127.0.0.1:2".parse().unwrap() };
    writer.register_peer(peer).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_remove_peer_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.register_peer(PeerEntry { peer_id: "p1".into(), discovery_addr: "127.0.0.1:1".parse().unwrap(), p2p_addr: "127.0.0.1:2".parse().unwrap() }).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_peer("p1".into()).unwrap();
    writer2.commit().unwrap();
}

#[test]
fn test_writer_insert_storage_change_set_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_storage_change_set(1, vec![(Address::repeat_byte(0x1), B256::repeat_byte(0x2), U256::from(1))]).unwrap();
    writer.commit().unwrap();
}

#[test]
fn test_writer_remove_change_set_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.insert_account_change_set(1, vec![(Address::repeat_byte(0x1), None)]).unwrap();
    writer.commit().unwrap();
    
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_change_set(1).unwrap();
    writer2.commit().unwrap();
}

#[test]
fn test_writer_remove_change_set_edge_not_found() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    writer.remove_change_set(1).unwrap();
    writer.commit().unwrap();
}
