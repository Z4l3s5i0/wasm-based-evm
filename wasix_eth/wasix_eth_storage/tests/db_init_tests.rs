use tempfile::NamedTempFile;
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_storage::codecs::Table;
use wasix_eth_types::genesis::GenesisConfiguration;
use wasix_eth_types::*;
use alloy_primitives::{Address, B256, U256, Bytes};
use std::collections::BTreeMap;
use redb::ReadableDatabase;

fn setup_db() -> EthDatabase {
    let tmp_file = NamedTempFile::new().unwrap();
    EthDatabase::open(tmp_file.path()).unwrap()
}

#[test]
fn test_db_init_tables() {
    let db = setup_db();
    // Tables are already initialized in open()
    let inner = db.inner();
    let tx = inner.begin_read().unwrap();
    // Verify some tables exist
    assert!(tx.open_table(wasix_eth_storage::tables::Headers::definition()).is_ok());
    assert!(tx.open_table(wasix_eth_storage::tables::Metadata::definition()).is_ok());
}

#[test]
fn test_genesis_init_standard() {
    let db = setup_db();
    let mut alloc = BTreeMap::new();
    let addr = Address::repeat_byte(0x11);
    alloc.insert(addr, GenesisAccount {
        balance: U256::from(1000),
        nonce: Some(1),
        code: Some(Bytes::from(vec![0x60, 0x01])),
        storage: Some(vec![(B256::repeat_byte(0x01), B256::from(U256::from(42)))].into_iter().collect()),
        ..Default::default()
    });

    let genesis = GenesisConfiguration {
        config: ChainConfig {
            chain_id: 1234,
            london_block: Some(0),
            shanghai_time: Some(0),
            ..Default::default()
        },
        alloc,
        coinbase: None,
        difficulty: None,
        extra_data: None,
        gas_limit: None,
        nonce: None,
        mix_hash: None,
        parent_hash: None,
        timestamp: None,
        number: None,
        gas_used: None,
        base_fee_per_gas: None,
        excess_blob_gas: None,
        blob_gas_used: None,
    };

    db.init_genesis(genesis).unwrap();

    let reader = DatabaseReadProvider::new(db.inner());
    
    // Check account
    let account = reader.account(addr, None).unwrap().unwrap();
    assert_eq!(account.balance, U256::from(1000));
    assert_eq!(account.nonce, 1);
    
    // Check storage
    assert_eq!(reader.storage(addr, B256::repeat_byte(0x01), None).unwrap(), U256::from(42));
    
    // Check metadata
    assert_eq!(reader.chain_id().unwrap(), 1234);
    assert!(reader.get_metadata("genesis_hash".to_string()).unwrap().is_some());
    
    // Check block 0
    let header = reader.header(BlockId::Number(0.into())).unwrap().unwrap();
    assert_eq!(header.number, 0);
    assert!(header.base_fee_per_gas.is_some());
    assert!(header.withdrawals_root.is_some());
    assert_eq!(header.withdrawals_root.unwrap(), wasix_eth_types::proofs::calculate_withdrawals_root(&[]));
    
    // Check withdrawals in body
    let body = reader.block_body(0).unwrap().unwrap();
    assert!(body.withdrawals.is_some());
    assert!(body.withdrawals.unwrap().is_empty());
}

#[test]
fn test_genesis_init_pre_shanghai() {
    let db = setup_db();
    let genesis = GenesisConfiguration {
        config: ChainConfig {
            chain_id: 1234,
            london_block: Some(0),
            shanghai_time: None,
            ..Default::default()
        },
        alloc: BTreeMap::new(),
        ..Default::default()
    };

    db.init_genesis(genesis).unwrap();
    let reader = DatabaseReadProvider::new(db.inner());
    let header = reader.header(BlockId::Number(0.into())).unwrap().unwrap();
    
    assert!(header.withdrawals_root.is_none());
    let body = reader.block_body(0).unwrap().unwrap();
    assert!(body.withdrawals.is_none());
}

#[test]
fn test_genesis_init_idempotent() {
    let db = setup_db();
    let genesis = GenesisConfiguration {
        config: ChainConfig::default(),
        alloc: BTreeMap::new(),
        coinbase: None,
        difficulty: None,
        extra_data: None,
        gas_limit: None,
        nonce: None,
        mix_hash: None,
        parent_hash: None,
        timestamp: None,
        number: None,
        gas_used: None,
        base_fee_per_gas: None,
        excess_blob_gas: None,
        blob_gas_used: None,
    };
    
    db.init_genesis(genesis.clone()).unwrap();
    let hash1 = db.begin_read().unwrap()
        .open_table(wasix_eth_storage::tables::Metadata::definition()).unwrap()
        .get("genesis_hash".to_string()).unwrap().unwrap().value();
    
    // Second init should do nothing
    db.init_genesis(genesis).unwrap();
    let hash2 = db.begin_read().unwrap()
        .open_table(wasix_eth_storage::tables::Metadata::definition()).unwrap()
        .get("genesis_hash".to_string()).unwrap().unwrap().value();
        
    assert_eq!(hash1, hash2);
}

#[test]
fn test_calculate_state_root_empty() {
    let db = setup_db();
    let root = db.calculate_state_root(false).unwrap();
    assert_eq!(root, alloy_trie::EMPTY_ROOT_HASH);
}

#[test]
fn test_genesis_init_pre_paris_difficulty() {
    let db = setup_db();
    let difficulty = U256::from(0x20000);
    let genesis = GenesisConfiguration {
        config: ChainConfig {
            chain_id: 1234,
            ..Default::default()
        },
        difficulty: Some(difficulty),
        alloc: BTreeMap::new(),
        ..Default::default()
    };

    db.init_genesis(genesis).unwrap();
    let reader = DatabaseReadProvider::new(db.inner());
    let header = reader.header(BlockId::Number(0.into())).unwrap().unwrap();
    
    assert_eq!(header.difficulty, difficulty);
}

#[test]
fn test_genesis_init_post_paris_difficulty() {
    let db = setup_db();
    let difficulty = U256::from(0x20000);
    let genesis = GenesisConfiguration {
        config: ChainConfig {
            chain_id: 1234,
            terminal_total_difficulty: Some(U256::ZERO),
            ..Default::default()
        },
        difficulty: Some(difficulty),
        alloc: BTreeMap::new(),
        ..Default::default()
    };

    db.init_genesis(genesis).unwrap();
    let reader = DatabaseReadProvider::new(db.inner());
    let header = reader.header(BlockId::Number(0.into())).unwrap().unwrap();
    
    // It should respect the difficulty in the genesis config even if TTD is 0
    assert_eq!(header.difficulty, difficulty);
}
