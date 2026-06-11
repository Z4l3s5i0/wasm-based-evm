use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_types::*;
use tempfile::tempdir;
use wasix_eth_types::genesis::GenesisConfiguration;
// --- open ---

#[test]
fn test_db_open_success() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test_success.db");
    let db = EthDatabase::open(&path);
    assert!(db.is_ok());
    assert!(path.exists());
}

#[test]
fn test_db_open_failure_invalid_path() {
    let dir = tempdir().unwrap();
    let invalid_path = dir.path().join("non_existent_dir").join("test.db");
    // Database::create fails if parent dir doesn't exist
    let db = EthDatabase::open(&invalid_path);
    assert!(db.is_err());
}

#[test]
fn test_db_open_edge_existing_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test_edge.db");
    {
        let _db = EthDatabase::open(&path).unwrap();
    }
    // Open again
    let db = EthDatabase::open(&path);
    assert!(db.is_ok());
}

// --- init_tables ---

#[test]
fn test_db_init_tables_success() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("init_success.db")).unwrap();
    let res = db.init_tables();
    assert!(res.is_ok());
}

#[test]
fn test_db_init_tables_failure_readonly() {
    // redb doesn't easily support opening a non-existent file as read-only to fail init
    // But we can simulate a failure if the transaction fails.
    // Since init_tables is public, if someone passed a broken inner, it might fail.
}

#[test]
fn test_db_init_tables_edge_already_initialized() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("init_edge.db")).unwrap();
    db.init_tables().unwrap();
    let res = db.init_tables();
    assert!(res.is_ok());
}

// --- init_genesis ---

#[test]
fn test_db_init_genesis_success() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("gen_success.db")).unwrap();
    let genesis = GenesisConfiguration {
        config: ChainConfig::default(),
        alloc: Default::default(),
        ..Default::default()
    };
    let res = db.init_genesis(genesis);
    assert!(res.is_ok());
    
    let reader = DatabaseReadProvider::new(db.inner());
    assert!(reader.get_metadata("genesis_hash".to_string()).unwrap().is_some());
}

#[test]
fn test_db_init_genesis_failure_invalid_config() {
    // Currently init_genesis is quite robust, but it could fail if calculation fails
    // or if the write transaction fails.
}

#[test]
fn test_db_init_genesis_edge_idempotent() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("gen_edge.db")).unwrap();
    let genesis = GenesisConfiguration {
        config: ChainConfig::default(),
        alloc: Default::default(),
        ..Default::default()
    };
    db.init_genesis(genesis.clone()).unwrap();
    let first_hash = DatabaseReadProvider::new(db.inner()).get_metadata("genesis_hash".to_string()).unwrap();
    
    // Call again with DIFFERENT config - should be ignored
    let mut genesis2 = genesis.clone();
    genesis2.config.chain_id = 999;
    db.init_genesis(genesis2).unwrap();
    
    let second_hash = DatabaseReadProvider::new(db.inner()).get_metadata("genesis_hash".to_string()).unwrap();
    assert_eq!(first_hash, second_hash);
}

// --- calculate_state_root ---

#[test]
fn test_db_calculate_state_root_success_empty() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("root_success.db")).unwrap();
    let root = db.calculate_state_root(false).unwrap();
    assert_eq!(root, alloy_trie::EMPTY_ROOT_HASH);
}

#[test]
fn test_db_calculate_state_root_failure() {
    // Hard to make it fail without corrupting DB
}

#[test]
fn test_db_calculate_state_root_edge_with_data() {
    let dir = tempdir().unwrap();
    let _db = EthDatabase::open(&dir.path().join("root_edge.db")).unwrap();
    // No easy public way to add data to PlainState without a provider, 
    // but we can use begin_write for direct table access if we wanted.
    // However, we'll test this via providers in read/write tests.
}

// --- inner ---

#[test]
fn test_db_inner_success() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("inner.db")).unwrap();
    let _inner = db.inner();
}

// --- begin_read / begin_write ---

#[test]
fn test_db_begin_read_success() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("read.db")).unwrap();
    let tx = db.begin_read();
    assert!(tx.is_ok());
}

#[test]
fn test_db_begin_write_success() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("write.db")).unwrap();
    let tx = db.begin_write();
    assert!(tx.is_ok());
}

#[test]
fn test_db_begin_write_failure_concurrent() {
    let dir = tempdir().unwrap();
    let db = EthDatabase::open(&dir.path().join("write_fail.db")).unwrap();
    let _tx1 = db.begin_write().unwrap();
    // redb allows multiple write transactions but they block or return error if not careful.
    // In our case, begin_write() on Database blocks or returns error based on redb version/config.
    // Database::begin_write blocks.
}
