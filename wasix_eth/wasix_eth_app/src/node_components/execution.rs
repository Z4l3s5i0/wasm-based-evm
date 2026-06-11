use std::sync::Arc;
use wasix_eth_core::account_manager::AccountManager;
use wasix_eth_core::engine::api::RPCEngine;
use wasix_eth_core::engine::forkchoice_validator::ForkchoiceValidator;
use wasix_eth_core::mempool::listener::MempoolListener;
use wasix_eth_core::mempool::mempool::Mempool;
use wasix_eth_core::{ChainManager, ChainManagerImpl, Engine, EthConsensus};
use wasix_eth_execution::execution_provider::EthExecutionProvider;
use wasix_eth_storage::{read::DatabaseReadProvider, write::DatabaseWriteProvider};
use wasix_eth_types::U256;

#[derive(Clone)]
pub struct ExecutionPayload {
    pub rpc_engine: Arc<RPCEngine>,
    pub engine: Arc<Engine>,
    pub mempool: Arc<Mempool>,
    pub chain_manager: Arc<dyn ChainManager>,
}

impl ExecutionPayload {
    pub fn new(
        read_provider: Arc<DatabaseReadProvider>,
        write_provider: Arc<DatabaseWriteProvider>,
    ) -> Self {
        let mempool = Arc::new(Mempool::new(U256::ZERO));
        let (engine_event_tx, _) = tokio::sync::broadcast::channel(100);

        let execution_provider = Arc::new(EthExecutionProvider::new((*read_provider).clone(), (*write_provider).clone()));
        let account_manager = Arc::new(AccountManager::new_with_dev_keys());
        let chain_manager = Arc::new(ChainManagerImpl::new((*read_provider).clone(), (*write_provider).clone()));
        let mempool_listener = Arc::new(MempoolListener::new(
            mempool.clone(),
            (*read_provider).clone(),
        ));
        let consensus = Arc::new(EthConsensus::new(read_provider.clone()));
        let forkchoice_validator = ForkchoiceValidator::new(
            (*read_provider).clone(),
            chain_manager.clone(),
        );
        let rpc_engine = Arc::new(RPCEngine::new(
            (*read_provider).clone(),
            (*write_provider).clone(),
            account_manager.clone(),
            chain_manager.clone(),
            mempool.clone(),
            engine_event_tx.clone(),
            consensus.clone(),
            execution_provider.clone()
        ));
        let engine = Arc::new(Engine::new(
            (*read_provider).clone(),
            (*write_provider).clone(),
            execution_provider,
            account_manager,
            chain_manager.clone(),
            mempool.clone(),
            engine_event_tx,
            consensus,
            rpc_engine.clone(),
            forkchoice_validator.clone(),
            mempool_listener
        ));

        Self {
            rpc_engine,
            engine,
            mempool,
            chain_manager,
        }
    }
}
