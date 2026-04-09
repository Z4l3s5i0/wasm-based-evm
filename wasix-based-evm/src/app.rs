use crate::cli::Args;
use crate::storage::genesis::Genesis;
use crate::storage::storage::InMemoryStorage;
use crate::executor::Executor;
use crate::rpc::{MyTransactionService, evm_rpc::transaction_service_server::TransactionServiceServer, DomainBlockchainProvider};
use crate::network::{self, NetworkConfig, NetworkHandle};
use crate::logging::{self, LogLevel};
use crate::ev::alloy_u256_to_evm_u256;
use alloy_primitives::U256;
use alloy_genesis::Genesis as AlloyGenesis;
use std::sync::Arc;
use crate::mempool::Mempool;
use tokio::sync::{Mutex, RwLock};
use tonic::transport::Server;
use std::path::PathBuf;
use crate::{debug, info};

pub struct App {
    rpc_addr: std::net::SocketAddr,
    transaction_service: MyTransactionService,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::default()
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        info!("[App] EVM gRPC Server listening on {}", self.rpc_addr);

        Server::builder()
            .add_service(TransactionServiceServer::new(self.transaction_service))
            .serve(self.rpc_addr)
            .await?;

        Ok(())
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
    network_handle: Option<NetworkHandle>,
    network_configured: bool,
    rpc_provider: Option<Box<dyn DomainBlockchainProvider + Send + Sync>>,
    rpc_provider_configured: bool,
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

    pub fn with_network_handle(mut self, network_handle: NetworkHandle) -> Self {
        self.network_handle = Some(network_handle);
        self.network_configured = true;
        self
    }

    pub fn with_rpc_provider(mut self, provider: Box<dyn DomainBlockchainProvider + Send + Sync>) -> Self {
        self.rpc_provider = Some(provider);
        self.rpc_provider_configured = true;
        self
    }

    pub fn with_logging(mut self, level: LogLevel) -> Self {
        logging::set_log_level(level);
        self.logging_configured = true;
        self
    }

    pub async fn build(self) -> Result<App, Box<dyn std::error::Error>> {
        let args = self.args.ok_or("Args not provided")?;

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
            self.genesis.ok_or("Genesis marked as configured but not provided")?
        } else {
            let genesis_path = data_dir.join("genesis/genesis.json");
            info!("[App] Loading genesis from {:?}", genesis_path);
            let genesis_file = std::fs::File::open(genesis_path)?;
            let alloy_genesis: AlloyGenesis = serde_json::from_reader(genesis_file)?;
            Genesis::from(alloy_genesis)
        };

        // 3. Storage, Mempool & Executor
        let storage = if self.storage_configured {
            self.storage.ok_or("Storage marked as configured but not provided")?
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
            self.mempool.ok_or("Mempool marked as configured but not provided")?
        } else {
            Arc::new(RwLock::new(Mempool::new(U256::ZERO)))
        };

        let executor = if self.executor_configured {
            self.executor.ok_or("Executor marked as configured but not provided")?
        } else {
            Executor::new()
        };

        // 4. Networking
        let network_handle = if self.network_configured {
            self.network_handle.ok_or("Network marked as configured but not provided")?
        } else {
            let network_config = NetworkConfig {
                discv5_addr: format!("0.0.0.0:{}", args.discovery_port).parse()?,
                p2p_addr: format!("0.0.0.0:{}", args.p2p_port).parse()?,
                ext_ip: args.ext_ip,
                bootnodes: args.bootnodes,
                max_peers: args.max_peers,
            };
            network::start_network(network_config, storage.clone(), mempool.clone()).await?
        };

        // 5. RPC Service
        let transaction_service = if self.rpc_provider_configured {
            let provider = self.rpc_provider.ok_or("RPC provider marked as configured but not provided")?;
            MyTransactionService::new(provider)
        } else {
            let pending_payloads = Arc::new(Mutex::new(std::collections::HashMap::new()));
            let provider = Box::new(crate::rpc::provider::DefaultBlockchainProvider {
                storage,
                mempool,
                executor,
                network_send: Some(network_handle.network_send.clone()),
                tx_broadcast: Some(network_handle.tx_broadcast.clone()),
                pending_payloads,
            });
            MyTransactionService::new(provider)
        };

        Ok(App {
            rpc_addr,
            transaction_service,
        })
    }
}
