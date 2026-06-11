use tempfile::NamedTempFile;
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_storage::write_traits::*;
use wasix_eth_types::*;
use alloy_primitives::{B256, Address, U256, Bytes};

fn setup_db() -> EthDatabase {
    let tmp_file = NamedTempFile::new().unwrap();
    let db = EthDatabase::open(tmp_file.path()).unwrap();
    db.init_tables().unwrap();
    db
}

#[test]
fn test_header_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let mut header = Header::default();
    header.number = 1;

    let hash = header.hash_slow();

    writer.insert_header(1, header.clone()).unwrap();
    writer.insert_block_hash(hash, 1).unwrap();
    writer.set_canonical(1, hash).unwrap();
    writer.commit().unwrap();

    let read_header = reader.header(BlockId::number(1)).unwrap().unwrap();
    assert_eq!(read_header.number, 1);

    let read_header_by_hash = reader.header(BlockId::hash(hash)).unwrap().unwrap();
    assert_eq!(read_header_by_hash.number, 1);

    assert_eq!(reader.block_number(hash).unwrap(), Some(1));
}

#[test]
fn test_block_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let number = 10;
    let body = BlockBody {
        transactions: vec![],
        ommers: vec![],
        withdrawals: None,
    };
    let mut header = Header::default();
    header.number = number;
    let hash = header.hash_slow();

    writer.insert_block_body(hash, number, body.clone()).unwrap();
    writer.set_canonical(number, hash).unwrap();
    writer.insert_block_hash(hash, number).unwrap();
    writer.insert_header(number, header.clone()).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.block_hash(number).unwrap(), Some(hash));
    assert_eq!(reader.block_number(hash).unwrap(), Some(number));
    
    let read_body = reader.block_body(number).unwrap().unwrap();
    assert_eq!(read_body.transactions.len(), 0);

    let read_body_by_hash = reader.block_body_by_hash(hash).unwrap().unwrap();
    assert_eq!(read_body_by_hash.transactions.len(), 0);

    let block = reader.block(BlockId::number(number)).unwrap().unwrap();
    assert_eq!(block.header.number, number);

    let block_by_hash = reader.block_by_hash(hash).unwrap().unwrap();
    assert_eq!(block_by_hash.header.number, number);

    assert_eq!(reader.latest_block_number().unwrap(), Some(number));

    // Test forkchoice
    let writer_fc = DatabaseWriteProvider::new(db.inner());
    writer_fc.update_forkchoice(hash, Some(hash), Some(hash)).unwrap();
    writer_fc.commit().unwrap();

    assert_eq!(reader.forkchoice("head").unwrap(), Some(hash));
    assert_eq!(reader.forkchoice("safe").unwrap(), Some(hash));
    assert_eq!(reader.forkchoice("finalized").unwrap(), Some(hash));

    // Test canonical removal
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_canonical(number).unwrap();
    writer2.commit().unwrap();
    assert_eq!(reader.block_hash(number).unwrap(), None);

    // Test payload
    let writer3 = DatabaseWriteProvider::new(db.inner());
    let payload_id = PayloadId::new([0x1; 8]);
    let block_for_payload = Block {
        header: header.clone(),
        body: body.clone(),
    };
    writer3.add_payload(payload_id, block_for_payload.clone(), vec![], BlobsBundleV1::default()).unwrap();
    writer3.commit().unwrap();

    let (p_block, p_receipts, _) = reader.get_payload(&payload_id).unwrap();
    assert_eq!(p_block.header.number, number);
    assert!(p_receipts.is_empty());

    let (p_block_by_hash, _, _) = reader.get_payload_by_block_hash(hash).unwrap();
    assert_eq!(p_block_by_hash.header.number, number);
}

#[test]
fn test_transaction_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let tx_inner = TxLegacy::default();
    let tx = Transaction::Legacy(Signed::new_unhashed(tx_inner, Signature::test_signature()));
    let hash = *tx.hash();
    let receipt = Receipt::default();

    let number = 100;
    let body = BlockBody {
        transactions: vec![tx.clone()],
        ommers: vec![],
        withdrawals: None,
    };
    let mut header = Header::default();
    header.number = number;
    let block_hash = header.hash_slow();

    writer.insert_transaction(hash, tx.clone()).unwrap();
    writer.insert_receipt(block_hash, 0, receipt.clone()).unwrap();
    writer.insert_transaction_lookup(hash, block_hash, 0).unwrap();
    writer.insert_block_body(block_hash, number, body).unwrap();
    writer.insert_header(number, header).unwrap();
    writer.insert_block_hash(block_hash, number).unwrap();
    writer.set_canonical(number, block_hash).unwrap();
    writer.commit().unwrap();

    assert!(reader.transaction(hash).unwrap().is_some());
    assert!(reader.transaction_receipt(hash).unwrap().is_some());
    assert_eq!(reader.transaction_block(hash).unwrap(), Some(number));
    assert_eq!(reader.block_hash(number).unwrap(), Some(block_hash));
    
    let (_block_num, _block_hash, _tx_idx) = reader.transaction_block_reference(hash).unwrap().unwrap();
    assert_eq!(_block_num, number);
    assert_eq!(_block_hash, block_hash);
    assert_eq!(_tx_idx, 0);
}

#[test]
fn test_account_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let address = Address::from([0xaa; 20]);
    let account = TrieAccount {
        nonce: 5,
        balance: U256::from(1000),
        storage_root: B256::from([0x44; 32]),
        code_hash: B256::from([0x55; 32]),
    };

    writer.update_account(address, account.clone()).unwrap();
    writer.commit().unwrap();

    let read_account = reader.account(address, None).unwrap().unwrap();
    assert_eq!(read_account.nonce, 5);
    assert_eq!(read_account.balance, U256::from(1000));

    let accounts = reader.accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].0, address);

    let addresses = reader.addresses().unwrap();
    assert_eq!(addresses.len(), 1);
    assert_eq!(addresses[0], address);

    assert_eq!(reader.transaction_count(address, BlockId::latest(), None).unwrap(), 5);

    // Test removal
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_account(address).unwrap();
    writer2.commit().unwrap();
    assert!(reader.account(address, None).unwrap().is_none());
}

#[test]
fn test_storage_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let address = Address::from([0xbb; 20]);
    let slot = B256::from([0x66; 32]);
    let value = U256::from(12345);

    writer.update_storage(address, slot, value).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.storage(address, slot, None).unwrap(), value);
    
    let storages = reader.account_storages(address, None).unwrap();
    assert_eq!(storages.len(), 1);
    assert_eq!(storages[0], (slot, value));
}

#[test]
fn test_bytecode_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let code = Bytes::from(vec![1, 2, 3, 4]);
    let hash = B256::from([0x77; 32]);

    writer.insert_bytecode(hash, code.clone()).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.bytecode(hash).unwrap(), Some(code));
}

#[test]
fn test_metadata_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let key = "test_key".to_string();
    let value = Bytes::from(vec![10, 20, 30]);

    writer.set_metadata(key.clone(), value.clone()).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.get_metadata(key).unwrap(), Some(value));
}

#[test]
fn test_change_set_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let number = 50;
    let address = Address::from([0xcc; 20]);
    let account_changes = vec![(address, Some(Bytes::from(vec![1, 2, 3])))];
    let storage_changes = vec![(address, B256::from([0x88; 32]), U256::from(999))];

    writer.insert_account_change_set(number, account_changes.clone()).unwrap();
    writer.insert_storage_change_set(number, storage_changes.clone()).unwrap();
    writer.commit().unwrap();

    let read_account_changes = reader.account_change_set(number).unwrap().unwrap();
    assert_eq!(read_account_changes.len(), 1);
    assert_eq!(read_account_changes[0].0, address);

    let read_storage_changes = reader.storage_change_set(number).unwrap().unwrap();
    assert_eq!(read_storage_changes.len(), 1);
    assert_eq!(read_storage_changes[0].0, address);
    assert_eq!(read_storage_changes[0].2, U256::from(999));

    // Test removal
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_change_set(number).unwrap();
    writer2.commit().unwrap();
    assert!(reader.account_change_set(number).unwrap().is_none());
    assert!(reader.storage_change_set(number).unwrap().is_none());
}

#[test]
fn test_state_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let address = Address::from([0xdd; 20]);
    let data = Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]);
    let hash = B256::from([0x99; 32]);
    let trie_hash = B256::from([0x88; 32]);

    writer.update_plain_state(address, data.clone()).unwrap();
    writer.update_hashed_state(hash, data.clone()).unwrap();
    writer.update_trie_node(trie_hash, data.clone()).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.plain_state(address).unwrap(), Some(data.clone()));
    assert_eq!(reader.hashed_state(hash).unwrap(), Some(data.clone()));
    assert_eq!(reader.trie_node(trie_hash).unwrap(), Some(data));

    // Test removal
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_plain_state(address).unwrap();
    writer2.commit().unwrap();
    assert!(reader.plain_state(address).unwrap().is_none());
}

#[test]
fn test_peer_discovery_provider_writer() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    let peer = PeerEntry {
        peer_id: "peer1".to_string(),
        discovery_addr: "127.0.0.1:30303".parse().unwrap(),
        p2p_addr: "127.0.0.1:30304".parse().unwrap(),
    };

    writer.register_peer(peer.clone()).unwrap();
    writer.commit().unwrap();

    let peers = reader.get_active_peers().unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].peer_id, "peer1");

    // Test removal
    let writer2 = DatabaseWriteProvider::new(db.inner());
    writer2.remove_peer("peer1".to_string()).unwrap();
    writer2.commit().unwrap();

    let peers_after = reader.get_active_peers().unwrap();
    assert_eq!(peers_after.len(), 0);
}

#[test]
fn test_chain_provider() {
    let db = setup_db();
    let writer = DatabaseWriteProvider::new(db.inner());
    let reader = DatabaseReadProvider::new(db.inner());

    // Default chain id (Sepolia-like in this codebase)
    assert_eq!(reader.chain_id().unwrap(), 31133);

    // Set custom chain id
    writer.set_metadata("chain_id".to_string(), Bytes::from(1234u64.to_be_bytes().to_vec())).unwrap();
    writer.commit().unwrap();

    assert_eq!(reader.chain_id().unwrap(), 1234);
}
