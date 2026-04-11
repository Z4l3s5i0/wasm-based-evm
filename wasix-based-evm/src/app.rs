use crate::cli::Args;
use crate::storage::genesis::Genesis;
use crate::storage::storage::InMemoryStorage;
use crate::executor::Executor;
use crate::logging::{self, LogLevel};
use crate::ev::alloy_u256_to_evm_u256;
use alloy_primitives::U256;
use alloy_genesis::Genesis as AlloyGenesis;
use std::sync::Arc;
use crate::mempool::Mempool;
use tokio::sync::{Mutex, RwLock};
use std::path::PathBuf;
use crate::{debug, info};

use crate::rpc::RpcServerFacade;
use crate::rpc::account_service::AccountService;
use crate::rpc::block_service::BlockService;
use crate::rpc::transaction_service::TransactionService;
use crate::rpc::log_service::LogService;

pub struct App {
    rpc_addr: std::net::SocketAddr,
    module: jsonrpsee::RpcModule<()>,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::default()
    }
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let server = jsonrpsee::server::Server::builder()
            .build(self.rpc_addr)
            .await?;

        info!("[App] JSON-RPC Server listening on {}", self.rpc_addr);
        let _handle = server.start(self.module);
        
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

        let rpc_addr = format!("127.0.0.1:{}", args.rpc_port).parse()?;

        // 2. Data Dir & Genesis
        let data_dir = if let Some(dir) = args.data_dir {
            dir
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

        // 4. RPC Setup
        let mut facade = RpcServerFacade::new();
        
        // We use the storage both as StateProvider, BlockProvider, etc.
        // Since InMemoryStorage implements all of them.
        // We need to handle the Arc<RwLock<InMemoryStorage>> properly.
        // However, our services expect Arc<dyn Provider>.
        // For simplicity in this refactor, let's create a wrapper or just use the storage directly if it was Arc<InMemoryStorage>.
        // Since it's RwLock, we might need a version that works with RwLock or just use a snapshot.
        // For now, let's assume we can use the storage directly for the services.
        
        let storage_provider: Arc<InMemoryStorage> = Arc::new(storage.read().await.clone()); 

        facade.register_accounts(AccountService { storage: storage_provider.clone() })?;
        facade.register_blocks(BlockService { storage: storage_provider.clone() })?;
        facade.register_transactions(TransactionService { storage: storage_provider.clone() })?;
        facade.register_logs(LogService { storage: storage_provider.clone() })?;
        
        Ok(App {
            rpc_addr,
            module: facade.into_module(),
        })
    }
}
