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

// --- HeaderProvider ---

#[test]
fn test_read_header_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let header = Header { number: 1, ..Default::default() };
    let hash = header.hash_slow();
    writer.insert_header(1, header.clone()).unwrap();
    writer.set_canonical(1, hash).unwrap();
    writer.commit().unwrap();
    
    let res = reader.header(BlockId::number(1)).unwrap().unwrap();
    assert_eq!(res.number, 1);
}

#[test]
fn test_read_header_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    let res = reader.header(BlockId::number(1)).unwrap();
    assert!(res.is_none());
}

#[test]
fn test_read_header_edge_hash_lookup() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let header = Header { number: 1, ..Default::default() };
    let hash = header.hash_slow();
    writer.insert_header(1, header).unwrap();
    writer.insert_block_hash(hash, 1).unwrap();
    writer.commit().unwrap();
    
    let res = reader.header(BlockId::hash(hash)).unwrap().unwrap();
    assert_eq!(res.number, 1);
}

// --- BlockProvider ---

#[test]
fn test_read_block_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let header = Header { number: 5, ..Default::default() };
    let body = BlockBody::<Transaction>::default();
    let hash = header.hash_slow();
    writer.insert_header(5, header).unwrap();
    writer.set_canonical(5, hash).unwrap();
    writer.insert_block_body(hash, 5, body).unwrap();
    writer.commit().unwrap();
    
    let res = reader.block(BlockId::number(5)).unwrap().unwrap();
    assert_eq!(res.header.number, 5);
}

#[test]
fn test_read_block_failure_missing_body() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let header = Header { number: 5, ..Default::default() };
    let hash = header.hash_slow();
    writer.insert_header(5, header).unwrap();
    writer.insert_block_hash(hash, 5).unwrap();
    writer.set_canonical(5, hash).unwrap();
    writer.commit().unwrap();
    
    let res = reader.block(BlockId::number(5)).unwrap();
    assert!(res.is_none()); // Should be None if body is missing
}

#[test]
fn test_read_block_edge_latest() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let header = Header { number: 10, ..Default::default() };
    let hash = header.hash_slow();
    writer.insert_header(10, header).unwrap();
    writer.insert_block_body(hash,10, BlockBody::<Transaction>::default()).unwrap();
    writer.set_canonical(10, hash).unwrap();
    writer.commit().unwrap();
    
    let res = reader.block(BlockId::latest()).unwrap().unwrap();
    assert_eq!(res.header.number, 10);
}

#[test]
fn test_read_block_hash_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    writer.set_canonical(1, hash).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.block_hash(1).unwrap(), Some(hash));
}

#[test]
fn test_read_block_hash_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.block_hash(1).unwrap(), None);
}

#[test]
fn test_read_block_hash_edge_zero() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.block_hash(0).unwrap(), None);
}

#[test]
fn test_read_block_number_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    writer.insert_block_hash(hash, 10).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.block_number(hash).unwrap(), Some(10));
}

#[test]
fn test_read_block_number_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.block_number(B256::from([0xee; 32])).unwrap(), None);
}

#[test]
fn test_read_block_number_edge_empty_hash() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.block_number(B256::ZERO).unwrap(), None);
}

#[test]
fn test_read_block_body_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::repeat_byte(0x1);
    writer.insert_block_body(hash, 1, BlockBody::<Transaction>::default()).unwrap();
    writer.set_canonical(1, hash).unwrap();
    writer.commit().unwrap();
    assert!(reader.block_body(1).unwrap().is_some());
}

#[test]
fn test_read_block_body_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.block_body(1).unwrap().is_none());
}

#[test]
fn test_read_block_body_edge_large_body() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let body = BlockBody::<Transaction> {
        transactions: vec![Transaction::Legacy(Signed::new_unhashed(TxLegacy::default(), Signature::test_signature())); 100],
        ommers: vec![],
        withdrawals: None,
    };
    let hash = B256::repeat_byte(0x1);
    writer.insert_block_body(hash, 1, body.clone()).unwrap();
    writer.set_canonical(1, hash).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.block_body(1).unwrap().unwrap().transactions.len(), 100);
}

#[test]
fn test_read_block_body_by_hash_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    writer.insert_block_hash(hash, 1).unwrap();
    writer.insert_block_body(hash, 1, BlockBody::<Transaction>::default()).unwrap();
    writer.commit().unwrap();
    assert!(reader.block_body_by_hash(hash).unwrap().is_some());
}

#[test]
fn test_read_block_body_by_hash_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.block_body_by_hash(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_block_body_by_hash_edge_no_lookup() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.insert_block_body(B256::repeat_byte(0x1), 1, BlockBody::<Transaction>::default()).unwrap();
    writer.commit().unwrap();
    // No insert_block_hash
    assert!(reader.block_body_by_hash(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_block_by_hash_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let header = Header { number: 1, ..Default::default() };
    let hash = header.hash_slow();
    writer.insert_header(1, header).unwrap();
    writer.insert_block_body(hash, 1, BlockBody::<Transaction>::default()).unwrap();
    writer.insert_block_hash(hash, 1).unwrap();
    writer.commit().unwrap();
    assert!(reader.block_by_hash(hash).unwrap().is_some());
}

#[test]
fn test_read_block_by_hash_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.block_by_hash(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_block_by_hash_edge_missing_parts() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    writer.insert_block_hash(hash, 1).unwrap();
    writer.commit().unwrap();
    // Missing header and body
    assert!(reader.block_by_hash(hash).unwrap().is_none());
}

#[test]
fn test_read_get_payload_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let id = PayloadId::new([1; 8]);
    let block = Block::<Transaction>::default();
    writer.add_payload(id, block.clone(), vec![], BlobsBundleV1::default()).unwrap();
    writer.commit().unwrap();
    assert!(reader.get_payload(&id).is_some());
}

#[test]
fn test_read_get_payload_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.get_payload(&PayloadId::new([0; 8])).is_none());
}

#[test]
fn test_read_get_payload_edge_large_receipts() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let id = PayloadId::new([2; 8]);
    let receipts = vec![Receipt::default(); 10];
    writer.add_payload(id, Block::<Transaction>::default(), receipts.clone(), BlobsBundleV1::default()).unwrap();
    writer.commit().unwrap();
    let (_, r, _) = reader.get_payload(&id).unwrap();
    assert_eq!(r.len(), 10);
}

#[test]
fn test_read_get_payload_by_block_hash_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let block = Block::<Transaction>::default();
    let hash = block.header.hash_slow();
    writer.add_payload(PayloadId::new([1; 8]), block, vec![], BlobsBundleV1::default()).unwrap();
    writer.commit().unwrap();
    assert!(reader.get_payload_by_block_hash(hash).is_some());
}

#[test]
fn test_read_get_payload_by_block_hash_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.get_payload_by_block_hash(B256::from([0xee; 32])).is_none());
}

#[test]
fn test_read_get_payload_by_block_hash_edge_duplicate_hash() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let block = Block::<Transaction>::default();
    let hash = block.header.hash_slow();
    writer.add_payload(PayloadId::new([1; 8]), block.clone(), vec![], BlobsBundleV1::default()).unwrap();
    writer.add_payload(PayloadId::new([2; 8]), block, vec![], BlobsBundleV1::default()).unwrap();
    writer.commit().unwrap();
    // Should still return one of them
    assert!(reader.get_payload_by_block_hash(hash).is_some());
}

#[test]
fn test_read_latest_block_number_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.set_canonical(100, B256::from([0x12; 32])).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.latest_block_number().unwrap(), Some(100));
}

#[test]
fn test_read_latest_block_number_failure_empty() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.latest_block_number().unwrap(), None);
}

#[test]
fn test_read_latest_block_number_edge_non_sequential() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.set_canonical(10, B256::from([0x12; 32])).unwrap();
    writer.set_canonical(5, B256::from([0x34; 32])).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.latest_block_number().unwrap(), Some(10));
}

#[test]
fn test_read_forkchoice_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    writer.update_forkchoice(hash, None, None).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.forkchoice("head").unwrap(), Some(hash));
}

#[test]
fn test_read_forkchoice_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.forkchoice("head").unwrap(), None);
}

#[test]
fn test_read_forkchoice_edge_unknown_key() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.forkchoice("unknown").unwrap(), None);
}

// --- TransactionProvider ---

#[test]
fn test_read_transaction_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let tx_inner = TxLegacy::default();
    let tx = Transaction::Legacy(Signed::new_unhashed(tx_inner, Signature::test_signature()));
    let hash = *tx.hash();
    writer.insert_transaction(hash, tx).unwrap();
    writer.commit().unwrap();
    assert!(reader.transaction(hash).unwrap().is_some());
}

#[test]
fn test_read_transaction_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.transaction(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_transaction_edge_zero_hash() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.transaction(B256::ZERO).unwrap().is_none());
}

#[test]
fn test_read_transaction_receipt_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    let block_hash = B256::from([0x34; 32]);
    writer.insert_block_hash(block_hash, 1).unwrap();
    writer.set_canonical(1, block_hash).unwrap();
    writer.insert_transaction_lookup(hash, block_hash, 0).unwrap();
    writer.insert_receipt(block_hash, 0, Receipt::default()).unwrap();
    writer.commit().unwrap();
    assert!(reader.transaction_receipt(hash).unwrap().is_some());
}

#[test]
fn test_read_transaction_receipt_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.transaction_receipt(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_transaction_receipt_edge_large_logs() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    let block_hash = B256::from([0x34; 32]);
    writer.insert_block_hash(block_hash, 1).unwrap();
    writer.set_canonical(1, block_hash).unwrap();
    writer.insert_transaction_lookup(hash, block_hash, 0).unwrap();
    let mut receipt: Receipt = Receipt::default();
    receipt.receipt.logs = vec![LogPrimitive::default(); 5];
    writer.insert_receipt(block_hash, 0, receipt).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.transaction_receipt(hash).unwrap().unwrap().receipt.logs.len(), 5);
}

#[test]
fn test_read_transaction_block_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    let block_hash = B256::from([0x34; 32]);
    writer.insert_block_hash(block_hash, 100).unwrap();
    writer.set_canonical(100, block_hash).unwrap();
    writer.insert_transaction_lookup(hash, block_hash, 0).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.transaction_block(hash).unwrap(), Some(100));
}

#[test]
fn test_read_transaction_block_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.transaction_block(B256::from([0xee; 32])).unwrap(), None);
}

#[test]
fn test_read_transaction_block_edge_multiple_lookups() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    let block_hash1 = B256::from([0x34; 32]);
    let block_hash2 = B256::from([0x56; 32]);
    writer.insert_block_hash(block_hash1, 100).unwrap();
    writer.set_canonical(100, block_hash1).unwrap();
    writer.insert_block_hash(block_hash2, 200).unwrap();
    writer.set_canonical(200, block_hash2).unwrap();
    writer.insert_transaction_lookup(hash, block_hash1, 0).unwrap();
    writer.insert_transaction_lookup(hash, block_hash2, 0).unwrap(); // Overwrite
    writer.commit().unwrap();
    assert_eq!(reader.transaction_block(hash).unwrap(), Some(200));
}

#[test]
fn test_read_transaction_block_reference_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let tx_inner = TxLegacy::default();
    let tx = Transaction::Legacy(Signed::new_unhashed(tx_inner, Signature::test_signature()));
    let tx_hash = *tx.hash();
    let block_hash = B256::from([0x12; 32]);
    writer.insert_block_hash(block_hash, 1).unwrap();
    writer.set_canonical(1, block_hash).unwrap();
    writer.insert_block_body(block_hash, 1, BlockBody::<Transaction> { transactions: vec![tx], ..Default::default() }).unwrap();
    writer.commit().unwrap();
    
    let (num, hash, idx) = reader.transaction_block_reference(tx_hash).unwrap().unwrap();
    assert_eq!(num, 1);
    assert_eq!(hash, block_hash);
    assert_eq!(idx, 0);
}

#[test]
fn test_read_transaction_block_reference_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.transaction_block_reference(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_transaction_block_reference_edge_missing_body() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let tx_hash = B256::from([0x12; 32]);
    let block_hash = B256::from([0x34; 32]);
    writer.insert_block_hash(block_hash, 1).unwrap();
    writer.set_canonical(1, block_hash).unwrap();
    writer.insert_transaction_lookup(tx_hash, block_hash, 0).unwrap();
    writer.commit().unwrap();
    // Now returns Some because we store (block_hash, index) in lookup
    assert!(reader.transaction_block_reference(tx_hash).unwrap().is_some());
}

// --- LogProvider ---

#[test]
fn test_read_logs_success_empty() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    let logs = reader.logs(Filter::default()).unwrap();
    assert!(logs.is_empty());
}

#[test]
fn test_read_logs_failure() {
    // Not implemented fully in the provider yet (it returns Result::Ok(vec![]))
}

#[test]
fn test_read_logs_edge() {
}

// --- AccountProvider ---

#[test]
fn test_read_account_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    let acc = TrieAccount { nonce: 5, balance: U256::from(100), ..Default::default() };
    writer.update_account(addr, acc.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.account(addr, None).unwrap(), Some(acc));
}

#[test]
fn test_read_account_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.account(Address::from([0xee; 20]), None).unwrap().is_none());
}

#[test]
fn test_read_account_edge_zero_address() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.account(Address::ZERO, None).unwrap().is_none());
}

#[test]
fn test_read_accounts_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.update_account(Address::from([0x12; 20]), TrieAccount::default()).unwrap();
    writer.update_account(Address::from([0x34; 20]), TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.accounts().unwrap().len(), 2);
}

#[test]
fn test_read_accounts_failure_empty() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.accounts().unwrap().is_empty());
}

#[test]
fn test_read_accounts_edge_large_set() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    for i in 0..100 {
        writer.update_account(Address::from([i as u8; 20]), TrieAccount::default()).unwrap();
    }
    writer.commit().unwrap();
    assert_eq!(reader.accounts().unwrap().len(), 100);
}

#[test]
fn test_read_addresses_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    writer.update_account(addr, TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.addresses().unwrap(), vec![addr]);
}

#[test]
fn test_read_addresses_failure_empty() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.addresses().unwrap().is_empty());
}

#[test]
fn test_read_addresses_edge_sorted() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let a1 = Address::from([1; 20]);
    let a2 = Address::from([2; 20]);
    writer.update_account(a2, TrieAccount::default()).unwrap();
    writer.update_account(a1, TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    let addrs = reader.addresses().unwrap();
    assert_eq!(addrs, vec![a1, a2]); // redb tables are sorted by key
}

#[test]
fn test_read_transaction_count_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    writer.update_account(addr, TrieAccount { nonce: 10, ..Default::default() }).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.transaction_count(addr, BlockId::latest(), None).unwrap(), 10);
}

#[test]
fn test_read_transaction_count_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.transaction_count(Address::from([0xee; 20]), BlockId::latest(), None).unwrap(), 0);
}

#[test]
fn test_read_transaction_count_edge_pending() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    // Currently ignores block_id
    assert_eq!(reader.transaction_count(Address::from([0xee; 20]), BlockId::pending(), None).unwrap(), 0);
}

#[test]
fn test_read_calculate_state_root_success() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.calculate_state_root(false, None).unwrap(), EMPTY_ROOT_HASH);
}

#[test]
fn test_read_calculate_state_root_failure() {
}

#[test]
fn test_read_calculate_state_root_edge_after_write() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.update_account(Address::from([0x12; 20]), TrieAccount::default()).unwrap();
    writer.commit().unwrap();
    assert_ne!(reader.calculate_state_root(false, None).unwrap(), EMPTY_ROOT_HASH);
}

#[test]
fn test_read_calculate_storage_root_success() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    assert_eq!(reader.calculate_storage_root(addr, None).unwrap(), EMPTY_ROOT_HASH);
}

#[test]
fn test_read_calculate_storage_root_failure() {
}

#[test]
fn test_read_calculate_storage_root_edge_with_storage() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    writer.update_storage(addr, B256::from([0x34; 32]), U256::from(1)).unwrap();
    writer.commit().unwrap();
    assert_ne!(reader.calculate_storage_root(addr, None).unwrap(), EMPTY_ROOT_HASH);
}

// --- ChainProvider ---

#[test]
fn test_read_chain_id_success_default() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.chain_id().unwrap(), 31133);
}

#[test]
fn test_read_chain_id_success_custom() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.set_metadata("chain_id".to_string(), 1234u64.to_be_bytes().to_vec().into()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.chain_id().unwrap(), 1234);
}

#[test]
fn test_read_chain_id_edge_invalid_data() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.set_metadata("chain_id".to_string(), vec![1, 2].into()).unwrap(); // Too short
    writer.commit().unwrap();
    // Should error if decoding fails
    assert!(reader.chain_id().is_err());
}

// --- StorageProvider ---

#[test]
fn test_read_storage_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    let slot = B256::from([0x34; 32]);
    writer.update_storage(addr, slot, U256::from(42)).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.storage(addr, slot, None).unwrap(), U256::from(42));
}

#[test]
fn test_read_storage_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert_eq!(reader.storage(Address::from([0x12; 20]), B256::from([0x34; 32]), None).unwrap(), U256::ZERO);
}

#[test]
fn test_read_storage_edge_overwrite() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    let slot = B256::from([0x34; 32]);
    writer.update_storage(addr, slot, U256::from(42)).unwrap();
    writer.update_storage(addr, slot, U256::from(43)).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.storage(addr, slot, None).unwrap(), U256::from(43));
}

#[test]
fn test_read_account_storages_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    writer.update_storage(addr, B256::from([1; 32]), U256::from(1)).unwrap();
    writer.update_storage(addr, B256::from([2; 32]), U256::from(2)).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.account_storages(addr, None).unwrap().len(), 2);
}

#[test]
fn test_read_account_storages_failure_empty() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.account_storages(Address::from([0xee; 20]), None).unwrap().is_empty());
}

#[test]
fn test_read_account_storages_edge_mixed_addresses() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let a1 = Address::from([1; 20]);
    let a2 = Address::from([2; 20]);
    writer.update_storage(a1, B256::from([0x34; 32]), U256::from(1)).unwrap();
    writer.update_storage(a2, B256::from([0x56; 32]), U256::from(2)).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.account_storages(a1, None).unwrap().len(), 1);
}

// --- BytecodeProvider ---

#[test]
fn test_read_bytecode_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let code = Bytes::from(vec![1, 2, 3]);
    let hash = alloy_primitives::keccak256(&code);
    writer.insert_bytecode(hash, code.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.bytecode(hash).unwrap().unwrap(), code);
}

#[test]
fn test_read_bytecode_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.bytecode(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_bytecode_edge_empty() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = alloy_primitives::keccak256(&[]);
    writer.insert_bytecode(hash, Bytes::default()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.bytecode(hash).unwrap().unwrap(), Bytes::default());
}

// --- StateProvider ---

#[test]
fn test_read_plain_state_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    let data = Bytes::from(vec![1, 2]);
    writer.update_plain_state(addr, data.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.plain_state(addr).unwrap().unwrap(), data);
}

#[test]
fn test_read_plain_state_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.plain_state(Address::from([0xee; 20])).unwrap().is_none());
}

#[test]
fn test_read_plain_state_edge_removal() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let addr = Address::from([0x12; 20]);
    writer.update_plain_state(addr, vec![1].into()).unwrap();
    writer.remove_plain_state(addr).unwrap();
    writer.commit().unwrap();
    assert!(reader.plain_state(addr).unwrap().is_none());
}

#[test]
fn test_read_hashed_state_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    let data = Bytes::from(vec![3, 4]);
    writer.update_hashed_state(hash, data.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.hashed_state(hash).unwrap().unwrap(), data);
}

#[test]
fn test_read_hashed_state_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.hashed_state(B256::from([0xee; 32])).unwrap().is_none());
}

#[test]
fn test_read_hashed_state_edge_large_data() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::from([0x12; 32]);
    let data = Bytes::from(vec![0; 1024]);
    writer.update_hashed_state(hash, data.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.hashed_state(hash).unwrap().unwrap().len(), 1024);
}

#[test]
fn test_read_trie_node_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::repeat_byte(0x1);
    let node = Bytes::from(vec![5, 6]);
    writer.update_trie_node(hash, node.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.trie_node(hash).unwrap().unwrap(), node);
}

#[test]
fn test_read_trie_node_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.trie_node(B256::repeat_byte(0x11)).unwrap().is_none());
}

#[test]
fn test_read_trie_node_edge_empty_hash() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let hash = B256::ZERO;
    writer.update_trie_node(hash, vec![1].into()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.trie_node(hash).unwrap().unwrap(), Bytes::from(vec![1]));
}

// --- MetadataProvider ---

#[test]
fn test_read_get_metadata_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.set_metadata("key".into(), vec![1].into()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.get_metadata("key".into()).unwrap().unwrap(), Bytes::from(vec![1]));
}

#[test]
fn test_read_get_metadata_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.get_metadata("none".into()).unwrap().is_none());
}

#[test]
fn test_read_get_metadata_edge_long_key() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let key = "a".repeat(256);
    writer.set_metadata(key.clone(), vec![2].into()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.get_metadata(key).unwrap().unwrap(), Bytes::from(vec![2]));
}

// --- PeerDiscoveryProvider ---

#[test]
fn test_read_active_peers_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let peer = PeerEntry { peer_id: "p1".into(), discovery_addr: "127.0.0.1:1".parse().unwrap(), p2p_addr: "127.0.0.1:2".parse().unwrap() };
    writer.register_peer(peer).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.get_active_peers().unwrap().len(), 1);
}

#[test]
fn test_read_active_peers_failure_empty() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.get_active_peers().unwrap().is_empty());
}

#[test]
fn test_read_active_peers_edge_multiple() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    writer.register_peer(PeerEntry { peer_id: "p1".into(), discovery_addr: "127.0.0.1:1".parse().unwrap(), p2p_addr: "127.0.0.1:2".parse().unwrap() }).unwrap();
    writer.register_peer(PeerEntry { peer_id: "p2".into(), discovery_addr: "127.0.0.1:3".parse().unwrap(), p2p_addr: "127.0.0.1:4".parse().unwrap() }).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.get_active_peers().unwrap().len(), 2);
}

// --- ChangeSetProvider ---

#[test]
fn test_read_account_change_set_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let changes = vec![(Address::from([0x12; 20]), Some(vec![1].into()))];
    writer.insert_account_change_set(1, changes.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.account_change_set(1).unwrap().unwrap(), changes);
}

#[test]
fn test_read_account_change_set_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.account_change_set(1).unwrap().is_none());
}

#[test]
fn test_read_account_change_set_edge_none_value() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let changes = vec![(Address::from([0x12; 20]), None)];
    writer.insert_account_change_set(1, changes.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.account_change_set(1).unwrap().unwrap(), changes);
}

#[test]
fn test_read_storage_change_set_success() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let changes = vec![(Address::from([0x12; 20]), B256::from([0x34; 32]), U256::from(1))];
    writer.insert_storage_change_set(1, changes.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.storage_change_set(1).unwrap().unwrap(), changes);
}

#[test]
fn test_read_storage_change_set_failure_not_found() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.storage_change_set(1).unwrap().is_none());
}

#[test]
fn test_read_storage_change_set_edge_multiple_entries() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());
    let changes = vec![
        (Address::from([0x12; 20]), B256::from([0x34; 32]), U256::from(1)),
        (Address::from([0x56; 20]), B256::from([0x78; 32]), U256::from(2)),
    ];
    writer.insert_storage_change_set(1, changes.clone()).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.storage_change_set(1).unwrap().unwrap().len(), 2);
}
