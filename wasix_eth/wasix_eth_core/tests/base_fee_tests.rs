
use wasix_eth_core::{Engine, EthConsensus};
use wasix_eth_types::{Header, ChainConfig, Hardfork, U256, Address, B256, Signature, TxLegacy, Bytes, BlockBody, Transaction, Receipt, Block, Bloom};
use std::sync::Arc;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::db::{EthDatabase};
use wasix_eth_core::mempool::mempool::Mempool;
use wasix_eth_core::account_manager::AccountManager;
use tokio::sync::broadcast;
use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_types::sync::SyncProvider;
use tempfile::tempdir;
use wasix_eth_core::ChainManagerImpl;

async fn setup_engine() -> (Engine, Arc<EthDatabase>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let db = EthDatabase::open(&db_path).unwrap();
    db.init_tables().unwrap();
    let db_arc = Arc::new(db);
    let read_storage = DatabaseReadProvider::new(db_arc.inner());
    let write_storage = DatabaseWriteProvider::new(db_arc.inner());
    let execution = Arc::new(EthExecutionProvider::new(read_storage.clone(), write_storage.clone()));
    let account_manager = Arc::new(AccountManager::new_with_dev_keys());
    let (event_tx, _) = broadcast::channel(10);
    let mempool = Arc::new(Mempool::new(U256::ZERO));
    let chain = Arc::new(ChainManagerImpl::new(read_storage.clone(), write_storage.clone()));

    let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
    (Engine::new(read_storage, write_storage, execution, account_manager, chain, mempool, event_tx, consensus), db_arc, dir)
}

#[tokio::test]
async fn test_base_fee_increase_small_delta() {
    let (engine, db, _dir) = setup_engine().await;
    
    // Setup chain config with London enabled
    let mut config = ChainConfig::default();
    config.london_block = Some(0);
    let config_json = serde_json::to_vec(&config).unwrap();
    
    use wasix_eth_storage::write_traits::MetadataWriter;
    engine.write_storage.set_metadata("chain_config".to_string(), config_json.into()).unwrap();
    engine.write_storage.clone().commit().unwrap();

    // parent_fee = 7, gas_used_delta = 1, gas_target = 10^7
    // fee_delta = 7 * 1 / 10^7 / 8 = 0
    let mut header = Header::default();
    header.number = 0;
    header.gas_limit = 20_000_000;
    header.gas_used = 10_000_001; // gas_target = 10_000_000, so delta = 1
    header.base_fee_per_gas = Some(7);

    let next_fee = engine.consensus.calculate_next_base_fee(&header, &config).unwrap();
    
    // Attempt to use alloy_consensus if possible
    // Note: alloy_consensus doesn't seem to have a top-level calculate_next_base_fee
    // but alloy_eips might have it.
    
    println!("Next fee: {}", next_fee);
    assert_eq!(next_fee, 7, "Base fee should not increase if delta rounds to 0 (Standard EIP-1559)");
}

#[tokio::test]
async fn test_first_london_block_fee() {
    let (engine, db, _dir) = setup_engine().await;
    
    // Setup chain config with London enabled at block 5
    let mut config = ChainConfig::default();
    config.london_block = Some(5);
    let config_json = serde_json::to_vec(&config).unwrap();
    
    use wasix_eth_storage::write_traits::MetadataWriter;
    engine.write_storage.set_metadata("chain_config".to_string(), config_json.into()).unwrap();
    engine.write_storage.clone().commit().unwrap();

    let mut header = Header::default();
    header.number = 4; // Parent of first London block
    header.gas_limit = 30_000_000;
    header.gas_used = 30_000_000; // Full block
    header.base_fee_per_gas = None; // Pre-London block

    let next_fee = engine.consensus.calculate_next_base_fee(&header, &config).unwrap();
    
    // The first London block should have exactly 1 Gwei, regardless of parent gas usage.
    assert_eq!(next_fee, 1_000_000_000, "First London block should have exactly 1 Gwei");
}
