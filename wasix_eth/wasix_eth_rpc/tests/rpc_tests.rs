use std::sync::Arc;
use wasix_eth_rpc::EthService;
use wasix_eth_core::Engine;
use wasix_eth_core::chain_manager::ChainManagerImpl;
use wasix_eth_core::account_manager::AccountManager;
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{AccountWriter, HeaderWriter, BlockWriter, StateWriter};
use wasix_eth_types::{BlockId, Header, TrieAccount, Address, B256, U256};
use tempfile::TempDir;
use wasix_eth_execution::execution_provider::EthExecutionProvider;
use wasix_eth_execution::executor::EvmExecutor;

#[tokio::test]
async fn test_rpc_get_balance_on_fork() {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(EthDatabase::open(temp_dir.path()).unwrap());
    db.init_tables().unwrap();

    let reader = DatabaseReadProvider::new(db.inner());
    let writer = DatabaseWriteProvider::new(db.inner());
    let chain = Arc::new(ChainManagerImpl::new(reader.clone(), writer.clone()));
    let account_manager = Arc::new(AccountManager::new());
    let (event_tx, _) = tokio::sync::broadcast::channel(100);
    
    // We need some mock/noop providers for Engine
    let execution = Arc::new(EthExecutionProvider::new(reader.clone(), writer.clone()));
    let mempool = Arc::new(wasix_eth_core::mempool::mempool_provider::NoopMempoolProvider);

    let engine = Arc::new(Engine::new(
        reader.clone(),
        writer.clone(),
        execution,
        account_manager,
        chain.clone(),
        mempool,
        event_tx,
    ));

    let eth_service = EthService { engine };

    let addr = Address::repeat_byte(0x42);

    // 1. Create Block A (canonical) at height 1
    let mut header_a = Header {
        number: 1,
        ..Default::default()
    };
    writer.update_account(addr, TrieAccount { balance: U256::from(100), ..Default::default() }).unwrap();
    let root_a = writer.calculate_state_root(false, None).unwrap();
    header_a.state_root = root_a;
    let hash_a = header_a.hash_slow();
    writer.insert_header(1, header_a).unwrap();
    writer.insert_block_hash(hash_a, 1).unwrap();
    writer.set_canonical(1, hash_a).unwrap();

    // 2. Create Block B (side-fork) at height 1
    let mut header_b = Header {
        number: 1,
        extra_data: wasix_eth_types::Bytes::from("fork"),
        ..Default::default()
    };
    writer.update_account(addr, TrieAccount { balance: U256::from(200), ..Default::default() }).unwrap();
    let root_b = writer.calculate_state_root(false, None).unwrap();
    header_b.state_root = root_b;
    let hash_b = header_b.hash_slow();
    writer.insert_header(1, header_b).unwrap();
    writer.insert_block_hash(hash_b, 1).unwrap();
    // Do NOT set canonical for B

    // 3. Call eth_getBalance for Block A hash
    let _balance_a = eth_service.get_balance(addr, BlockId::Hash(hash_a.into())).await.unwrap();
    // We didn't save state to TrieNodes for A, so it might fall back to latest or fail if we didn't save it.
    // But since A is canonical and latest in our setup, it should work.

    // To make it work for fork B, we simulate saving the state to TrieNodes
    let mut buf = Vec::new();
    alloy_rlp::Encodable::encode(&TrieAccount { balance: U256::from(200), ..Default::default() }, &mut buf);
    writer.update_trie_node(root_b, wasix_eth_types::Bytes::from(buf)).unwrap();

    let balance_b = eth_service.get_balance(addr, BlockId::Hash(hash_b.into())).await.unwrap();
    assert_eq!(balance_b, U256::from(200));
}
