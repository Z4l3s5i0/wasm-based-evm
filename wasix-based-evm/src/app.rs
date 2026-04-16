use std::net::SocketAddr;
use crate::cli::Args;
use crate::storage::genesis::Genesis;
use crate::storage::storage::{InMemoryStorage, StorageProvider};
use crate::executor::Executor;
use crate::logging::{self, LogLevel};
use crate::ev::alloy_u256_to_evm_u256;
use alloy_primitives::U256;
use alloy_genesis::Genesis as AlloyGenesis;
use std::sync::Arc;
use crate::mempool::Mempool;
use crate::p2p::identity::Identity;
use crate::p2p::peer_manager::PeerManager;
use crate::{info, debug, error};

use crate::rpc::account_manager::AccountManager;
use tokio::sync::RwLock;
use std::path::PathBuf;
use crate::rpc::RpcServerFacade;
use crate::rpc::eth_service::EthService;
use crate::rpc::debug_service::DebugService;
use crate::rpc::account_service::AccountService;
use crate::rpc::block_service::BlockService;
use crate::rpc::transaction_service::TransactionService;
use crate::rpc::log_service::LogService;
use crate::rpc::engine_service::EngineService;
use crate::p2p::gossip_handler::GossipHandler;

use crate::p2p::sync::SyncEngine;

pub struct App {
    eth_rpc_addr: std::net::SocketAddr,
    auth_rpc_addr: std::net::SocketAddr,
    frontend_addr: std::net::SocketAddr,
    eth_rpc_port: u16,
    eth_module: jsonrpsee::RpcModule<()>,
    auth_module: jsonrpsee::RpcModule<()>,
    swarm: Arc<PeerManager>,
    gossip_rx: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    mempool: Arc<RwLock<Mempool>>,
    storage: Arc<RwLock<InMemoryStorage>>,
    executor: Arc<Executor>,
    data_dir: PathBuf,
    dev_interval: Option<u64>,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::default()
    }
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let bootnodes = self.swarm.bootnodes.clone();
        let discovery_port = self.swarm.discovery_port;
        let swarm = self.swarm.clone();
        tokio::spawn(async move {
            if let Err(e) = swarm.start(discovery_port, bootnodes).await {
                error!("[App] Peer Manager error: {}", e);
            }
        });


        let eth_server = jsonrpsee::server::Server::builder()
            .build(self.eth_rpc_addr)
            .await?;
        
        let auth_server = jsonrpsee::server::Server::builder()
            .build(self.auth_rpc_addr)
            .await?;

        info!("[App] Eth JSON-RPC Server listening on {}", self.eth_rpc_addr);
        info!("[App] Auth Engine JSON-RPC Server listening on {}", self.auth_rpc_addr);
        
        // let frontend_addr = self.frontend_addr;
        let eth_rpc_port = self.eth_rpc_port;
        // tokio::spawn(async move {
        //     if let Err(e) = crate::frontend::start_frontend(frontend_addr, eth_rpc_port).await {
        //         error!("[App] Frontend error: {}", e);
        //     }
        // });

        let _eth_handle = eth_server.start(self.eth_module);
        let _auth_handle = auth_server.start(self.auth_module);

        if let Some(interval) = self.dev_interval {
            let dev_mode = crate::dev::DevMode::new(
                self.mempool.clone(),
                self.storage.clone(),
                self.executor.clone(),
                interval,
            );
            tokio::spawn(async move {
                dev_mode.start().await;
            });
        }

        let storage = self.storage.clone();
        let data_dir = self.data_dir.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                debug!("[App] Periodic state dump...");
                let storage_inner = storage.read().await;
                if let Err(e) = storage_inner.save_to_file(data_dir.join("state.json")) {
                    error!("[App] Failed to dump state: {}", e);
                }
            }
        });
        
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
}

#[derive(Default)]
pub struct AppBuilder {
    args: Option<Args>,
    genesis: Option<Genesis>,
    genesis_configured: bool,
    storage: Option<Arc<RwLock<InMemoryStorage>>>,
    mempool: Option<Arc<RwLock<Mempool>>>,
    storage_configured: bool,
    mempool_configured: bool,
    executor: Option<Executor>,
    executor_configured: bool,
    logging_configured: bool,
    account_manager: Option<Arc<AccountManager>>,
}

impl AppBuilder {
    pub fn with_config(mut self, args: Args) -> Self {
        self.args = Some(args);
        self
    }

    pub fn with_genesis(mut self, genesis: Genesis) -> Self {
        self.genesis = Some(genesis);
        self.genesis_configured = true;
        self
    }

    pub fn with_storage(mut self, storage: Arc<RwLock<InMemoryStorage>>) -> Self {
        self.storage = Some(storage);
        self.storage_configured = true;
        self
    }

    pub fn with_mempool(mut self, mempool: Arc<RwLock<Mempool>>) -> Self {
        self.mempool = Some(mempool);
        self.mempool_configured = true;
        self
    }

    pub fn with_executor(mut self, executor: Executor) -> Self {
        self.executor = Some(executor);
        self.executor_configured = true;
        self
    }

    pub fn with_logging(mut self, level: LogLevel) -> Self {
        logging::set_log_level(level);
        self.logging_configured = true;
        self
    }

    pub fn with_account_manager(mut self, manager: AccountManager) -> Self {
        self.account_manager = Some(Arc::new(manager));
        self
    }

    pub async fn build(self) -> Result<App, Box<dyn std::error::Error>> {
        let args = self.args.clone().ok_or("Args not provided")?;

        if !self.logging_configured {
            let level = match args.verbose {
                0 => LogLevel::None,
                1 => LogLevel::Info,
                _ => LogLevel::Debug,
            };
            logging::set_log_level(level);
        }

        let bind_ip = args.ext_ip.unwrap_or_else(|| "127.0.0.1".parse().unwrap());
        let eth_rpc_addr = SocketAddr::new(bind_ip, args.eth_rpc_port);
        let auth_rpc_addr = SocketAddr::new(bind_ip, args.auth_rpc_port);
        let frontend_addr = SocketAddr::new(bind_ip, args.frontend_port);

        let data_dir = if let Some(ref dir) = args.data_dir {
            if !dir.exists() {
                std::fs::create_dir_all(dir)?;
            }
            dir.clone()
        } else if cfg!(target_os = "wasi") {
            std::env::current_dir()?
        } else {
            PathBuf::from(std::env!("CARGO_MANIFEST_DIR"))
        };

        let storage_path = data_dir.join("storage/state.json");
        let storage = if self.storage_configured {
            self.storage.clone().ok_or("Storage marked as configured but not provided")?
        } else if storage_path.exists() {
            info!("[App] Loading existing state from {:?}", storage_path);
            let storage_inner = InMemoryStorage::load_from_file(storage_path)?;
            Arc::new(RwLock::new(storage_inner))
        } else {
            let genesis = if self.genesis_configured {
                self.genesis.clone().ok_or("Genesis marked as configured but not provided")?
            } else {
                let genesis_path = data_dir.join("genesis/genesis.json");
                info!("[App] Loading genesis from {:?}", genesis_path);
                let genesis_file = std::fs::File::open(genesis_path)?;
                let alloy_genesis: AlloyGenesis = serde_json::from_reader(genesis_file)?;
                Genesis::from(alloy_genesis)
            };
            let chain_id = alloy_u256_to_evm_u256(U256::from(genesis.chain_id));
            let storage_inner = InMemoryStorage::new_with_genesis(chain_id, genesis);
            Arc::new(RwLock::new(storage_inner))
        };

        let mempool = if self.mempool_configured {
            self.mempool.clone().ok_or("Mempool marked as configured but not provided")?
        } else {
            Arc::new(RwLock::new(Mempool::new(U256::ZERO)))
        };

        let executor = if self.executor_configured {
            self.executor.clone().ok_or("Executor marked as configured but not provided")?
        } else {
            Executor::new()
        };
        let executor = Arc::new(executor);

        let account_manager = if let Some(manager) = self.account_manager {
            manager
        } else {
            Arc::new(AccountManager::new_with_dev_keys())
        };

        let p2p_identity = Identity::new(
            args.data_dir.as_deref(),
        )?;
        info!("[App] P2P Identity generated. PeerId: {}", p2p_identity.peer_id());

        let (peer_manager, gossip_rx) = PeerManager::new(
            p2p_identity,
            storage.clone(),
            args.discovery_port,
            args.p2p_port,
            args.ext_ip,
            args.bootnodes.clone(),
        )?;
        let peer_manager = Arc::new(peer_manager);

        let sync_engine = Arc::new(SyncEngine::new(
            storage.clone(),
            mempool.clone(),
            peer_manager.clone(),
            executor.clone(),
        ));
        let sync_handle = sync_engine.clone();
        tokio::spawn(async move {
            sync_handle.start().await;
        });

        let gossip_handler = GossipHandler::new(
            mempool.clone(),
            peer_manager.clone(),
            sync_engine.clone(),
            gossip_rx,
        );
        tokio::spawn(async move {
            gossip_handler.start().await;
        });

        let mut eth_facade = RpcServerFacade::new();
        let mut auth_facade = RpcServerFacade::new();
        let provider = Arc::new(StorageProvider::new(storage.clone(), mempool.clone(), executor.clone()));

        eth_facade.register_accounts(AccountService { storage: provider.clone() })?;
        eth_facade.register_debug(DebugService { mempool: mempool.clone() })?;
        eth_facade.register_eth(EthService { 
            block_storage: provider.clone(),
            state_storage: provider.clone(),
            mempool: mempool.clone(),
            peer_manager: peer_manager.clone(),
            executor: (*executor).clone(),
            storage: storage.clone(),
            account_manager: account_manager.clone(),
            sync_engine: sync_engine.clone(),
        })?;
        auth_facade.register_engine(EngineService::new(
            storage.clone(),
            mempool.clone(),
            (*executor).clone(),
            sync_engine.clone(),
        ))?;
        eth_facade.register_blocks(BlockService { storage: provider.clone() })?;
        eth_facade.register_transactions(TransactionService { storage: provider.clone() })?;
        eth_facade.register_logs(LogService { storage: provider.clone() })?;

        Ok(App {
            eth_rpc_addr,
            auth_rpc_addr,
            frontend_addr,
            eth_rpc_port: args.eth_rpc_port,
            eth_module: eth_facade.into_module(),
            auth_module: auth_facade.into_module(),
            swarm: peer_manager,
            gossip_rx: None, // Moved to GossipHandler
            mempool: mempool.clone(),
            storage: storage.clone(),
            executor: executor.clone(),
            data_dir,
            dev_interval: args.dev,
        })
    }
}
