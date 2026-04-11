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
use crate::p2p::discovery::DiscoveryService;
use crate::p2p::swarm::SwarmService;
use crate::rpc::account_manager::AccountManager;
use tokio::sync::RwLock;
use std::path::PathBuf;
use crate::{debug, info};

use crate::rpc::RpcServerFacade;
use crate::rpc::eth_service::EthService;
use crate::rpc::account_service::AccountService;
use crate::rpc::block_service::BlockService;
use crate::rpc::transaction_service::TransactionService;
use crate::rpc::log_service::LogService;
use crate::rpc::engine_service::EngineService;

pub struct App {
    eth_rpc_addr: std::net::SocketAddr,
    auth_rpc_addr: std::net::SocketAddr,
    eth_module: jsonrpsee::RpcModule<()>,
    auth_module: jsonrpsee::RpcModule<()>,
    discovery: DiscoveryService,
    swarm: SwarmService,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::default()
    }
    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("[App] Node ENR: {}", self.discovery.local_enr());

        // Start P2P discovery
        self.discovery.start().await?;

        // Start libp2p swarm
        self.swarm.start().await?;

        let eth_server = jsonrpsee::server::Server::builder()
            .build(self.eth_rpc_addr)
            .await?;
        
        let auth_server = jsonrpsee::server::Server::builder()
            .build(self.auth_rpc_addr)
            .await?;

        info!("[App] Eth JSON-RPC Server listening on {}", self.eth_rpc_addr);
        info!("[App] Auth Engine JSON-RPC Server listening on {}", self.auth_rpc_addr);
        
        let _eth_handle = eth_server.start(self.eth_module);
        let _auth_handle = auth_server.start(self.auth_module);
        
        // Keep the app running
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

        // 1. Logging
        if !self.logging_configured {
            let level = match args.verbose {
                0 => LogLevel::None,
                1 => LogLevel::Info,
                _ => LogLevel::Debug,
            };
            logging::set_log_level(level);
        }

        let eth_rpc_addr = format!("127.0.0.1:{}", args.eth_rpc_port).parse()?;
        let auth_rpc_addr = format!("127.0.0.1:{}", args.auth_rpc_port).parse()?;

        // 2. Data Dir & Genesis
        let data_dir = if let Some(ref dir) = args.data_dir {
            dir.clone()
        } else if cfg!(target_os = "wasi") {
            std::env::current_dir()?
        } else {
            PathBuf::from(std::env!("CARGO_MANIFEST_DIR"))
        };

        let genesis = if self.genesis_configured {
            self.genesis.clone().ok_or("Genesis marked as configured but not provided")?
        } else {
            let genesis_path = data_dir.join("genesis/genesis.json");
            info!("[App] Loading genesis from {:?}", genesis_path);
            let genesis_file = std::fs::File::open(genesis_path)?;
            let alloy_genesis: AlloyGenesis = serde_json::from_reader(genesis_file)?;
            Genesis::from(alloy_genesis)
        };

        // 3. Storage, Mempool & Executor
        let storage = if self.storage_configured {
            self.storage.clone().ok_or("Storage marked as configured but not provided")?
        } else {
            let chain_id = alloy_u256_to_evm_u256(U256::from(genesis.chain_id));
            let storage_inner = InMemoryStorage::new_with_genesis(chain_id, genesis);
            // Log pre-funded accounts for clarity
            for (h160, account) in &storage_inner.backend.state {
                debug!("[App] Pre-funded account: 0x{:x}, balance: {} wei", h160, account.balance);
            }
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

        let account_manager = if let Some(manager) = self.account_manager {
            manager
        } else {
            // By default, enable dev keys for easier testing if we are in dev mode/debug
            Arc::new(AccountManager::new_with_dev_keys())
        };

        // 4. RPC Setup
        let mut eth_facade = RpcServerFacade::new();
        let mut auth_facade = RpcServerFacade::new();
        
        // Use the StorageProvider wrapper to handle the Arc<RwLock<InMemoryStorage>>
        // This allows RPC services to see live updates from the Executor.
        let provider = Arc::new(StorageProvider::new(storage.clone()));

        eth_facade.register_accounts(AccountService { storage: provider.clone() })?;
        eth_facade.register_eth(EthService { 
            block_storage: provider.clone(),
            state_storage: provider.clone(),
            mempool: mempool.clone(),
            executor: executor.clone(),
            storage: storage.clone(),
            account_manager: account_manager.clone(),
        })?;
        auth_facade.register_engine(EngineService::new(
            storage.clone(),
            mempool.clone(),
            executor.clone(),
        ))?;
        eth_facade.register_blocks(BlockService { storage: provider.clone() })?;
        eth_facade.register_transactions(TransactionService { storage: provider.clone() })?;
        eth_facade.register_logs(LogService { storage: provider.clone() })?;

        // 5. P2P Identity
        let p2p_identity = Identity::new(
            args.data_dir.as_deref(),
            args.p2p_port,
            args.discovery_port,
            args.ext_ip,
        )?;
        info!("[App] P2P Identity generated: {}", p2p_identity.enr);

        // 6. Discovery Service
        let discovery = DiscoveryService::new(
            &p2p_identity.keypair,
            p2p_identity.enr,
            args.discovery_port,
            args.bootnodes.clone(),
        ).await?;

        // 7. Swarm Service
        let swarm = SwarmService::new(
            &p2p_identity.keypair,
            args.p2p_port,
        )?;
        
        Ok(App {
            eth_rpc_addr,
            auth_rpc_addr,
            eth_module: eth_facade.into_module(),
            auth_module: auth_facade.into_module(),
            discovery,
            swarm,
        })
    }
}
