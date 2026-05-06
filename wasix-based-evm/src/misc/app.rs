use std::net::SocketAddr;
use crate::cli::Args;
use crate::storage::storage::{RedbStorage, StorageProvider, GenesisInit};
use alloy_primitives::U256;
use std::sync::Arc;
use crate::mempool::Mempool;
use crate::p2p::peer_manager::PeerManager;
use crate::{info, debug, error};

use crate::rpc::account_manager::AccountManager;
use tokio::sync::RwLock;
use std::path::PathBuf;
use axum::Router;
use axum::routing::get;
use jsonrpsee::server::middleware::rpc::RpcServiceBuilder;
use prometheus::{Encoder, TextEncoder};
use crate::evm::ev::alloy_u256_to_evm_u256;
use crate::evm::executor::Executor;
use crate::identity::identity::Identity;
use crate::identity::jwt::{HeaderInjectorLayer, JwtAuthLayer};
use crate::misc::logging;
use crate::misc::logging::LogLevel;
use crate::rpc::RpcServerFacade;
use crate::rpc::eth_service::EthService;
use crate::rpc::debug_service::DebugService;
use crate::rpc::engine_service::EngineService;
use crate::p2p::gossip_handler::GossipHandler;
use crate::sync::controller::SyncController;

pub struct App {
    eth_rpc_addr: SocketAddr,
    auth_rpc_addr: SocketAddr,
    frontend_addr: SocketAddr,
    eth_rpc_port: u16,
    eth_module: jsonrpsee::RpcModule<()>,
    auth_module: jsonrpsee::RpcModule<()>,
    swarm: Arc<PeerManager>,
    gossip_rx: Option<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    mempool: Arc<RwLock<Mempool>>,
    storage: Arc<RwLock<RedbStorage>>,
    executor: Arc<Executor>,
    data_dir: PathBuf,
    state_filename: String,
    dev_interval: Option<u64>,
    auth_rpc_jwt_secret: Option<[u8; 32]>,
    metrics_port: u16
}

async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
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
        
        let rpc_middleware = RpcServiceBuilder::new();
        let http_middleware = tower::ServiceBuilder::new()
            .layer(HeaderInjectorLayer);
        let _auth_handle = if let Some(secret) = self.auth_rpc_jwt_secret {
            info!("[App] Enabling JWT authentication for Auth Engine JSON-RPC");
            let auth_server = jsonrpsee::server::Server::builder()
                .set_rpc_middleware(rpc_middleware.layer(JwtAuthLayer::new(secret)))
                .set_http_middleware(http_middleware)
                .build(self.auth_rpc_addr)
                .await?;
            auth_server.start(self.auth_module)
        } else {
            error!("[App] JWT authentication key missing: Auth RPC requires JWT authentication");
            return Err("JWT authentication key missing for Auth RPC".into());
        };

        info!("[App] Eth JSON-RPC Server listening on {}", self.eth_rpc_addr);
        info!("[App] Auth Engine JSON-RPC Server listening on {}", self.auth_rpc_addr);
        
        // let frontend_addr = self.frontend_addr;
        let _eth_rpc_port = self.eth_rpc_port;
        // tokio::spawn(async move {
        //     if let Err(e) = crate::frontend::start_frontend(frontend_addr, eth_rpc_port).await {
        //         error!("[App] Frontend error: {}", e);
        //     }
        // });

        let _eth_handle = eth_server.start(self.eth_module);

        // Start metrics server
        crate::misc::metrics::init_metrics();
        let metrics_addr: SocketAddr = format!("0.0.0.0:{}", self.metrics_port).parse().unwrap();
        let metrics_app: Router = Router::new().route("/metrics", get(metrics_handler));
        info!("[App] Prometheus metrics server listening on {}", metrics_addr);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(metrics_addr).await.unwrap();
            axum::serve(listener, metrics_app).await.unwrap();
        });

        // Node Uptime & Block Import Rate Background Task
        tokio::spawn(async move {
            let mut last_height = 0.0;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                crate::misc::metrics::NODE_UPTIME.inc();
                
                let current_height = crate::misc::metrics::CURRENT_HEAD_BLOCK.get();
                if current_height > 0.0 {
                    if last_height > 0.0 {
                        let rate = current_height - last_height;
                        crate::misc::metrics::BLOCK_IMPORT_RATE.set(rate);
                    }
                    last_height = current_height;
                }
            }
        });

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
        let state_filename = self.state_filename.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(90)).await;
                debug!("[App] Periodic state dump...");
                let storage_json = {
                    let storage_inner = storage.read().await;
                    storage_inner.save_to_vec_pretty()
                };
                
                match storage_json {
                    Ok(json) => {
                        if let Err(e) = std::fs::write(data_dir.join(&state_filename), json) {
                            error!("[App] Failed to dump state to file: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("[App] Failed to serialize state: {}", e);
                    }
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
    genesis: Option<GenesisInit>,
    genesis_configured: bool,
    storage: Option<Arc<RwLock<RedbStorage>>>,
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

    pub fn with_genesis(mut self, genesis: GenesisInit) -> Self {
        self.genesis = Some(genesis);
        self.genesis_configured = true;
        self
    }

    pub fn with_storage(mut self, storage: Arc<RwLock<RedbStorage>>) -> Self {
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

    async fn setup_logging(&self, args: &Args) {
        if !self.logging_configured {
            let level = match args.verbose {
                0 => LogLevel::None,
                1 => LogLevel::Info,
                _ => LogLevel::Debug,
            };
            logging::set_log_level(level);
        }
    }

    fn setup_addresses(&self, args: &Args) -> (SocketAddr, SocketAddr, SocketAddr) {
        let bind_ip = args.ext_ip.unwrap_or_else(|| "127.0.0.1".parse().unwrap());
        let eth_rpc_addr = SocketAddr::new(bind_ip, args.eth_rpc_port);
        let auth_rpc_addr = SocketAddr::new(bind_ip, args.auth_rpc_port);
        let frontend_addr = SocketAddr::new(bind_ip, args.frontend_port);
        (eth_rpc_addr, auth_rpc_addr, frontend_addr)
    }

    fn setup_data_dir(&self, args: &Args) -> Result<PathBuf, Box<dyn std::error::Error>> {
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
        Ok(data_dir)
    }

    fn setup_account_manager(&self) -> Arc<AccountManager> {
        if let Some(ref manager) = self.account_manager {
            manager.clone()
        } else {
            Arc::new(AccountManager::new_with_dev_keys())
        }
    }

    fn setup_genesis(&self, args: &Args, data_dir: &PathBuf) -> Result<GenesisInit, Box<dyn std::error::Error>> {
        if self.genesis_configured {
            self.genesis.clone().ok_or("Genesis marked as configured but not provided".into())
        } else {
            let genesis_path = args.genesis_path.clone().unwrap_or_else(|| data_dir.join("genesis.json"));
            info!("[App] Loading genesis from {:?}", genesis_path);
            let genesis_file = std::fs::File::open(genesis_path)?;
            let genesis_init: GenesisInit = serde_json::from_reader(genesis_file)?;
            Ok(genesis_init)
        }
    }

    async fn setup_storage(&self, args: &Args, data_dir: &PathBuf, peer_id: &str) -> Result<(Arc<RwLock<RedbStorage>>, String), Box<dyn std::error::Error>> {
        let storage_name = args.peer_name.clone().unwrap_or_else(|| peer_id.to_string());
        let state_filename = format!("state_{}.txt", storage_name);
        let storage_path = data_dir.join(&state_filename);

        let storage = if self.storage_configured {
            self.storage.clone().ok_or("Storage marked as configured but not provided")?
        } else if storage_path.exists() {
            info!("[App] Loading existing state from {:?}", storage_path);
            let storage_inner = RedbStorage::load_from_file(storage_path)?;
            Arc::new(RwLock::new(storage_inner))
        } else {
            let genesis_init = self.setup_genesis(args, data_dir)?;
            let chain_id = alloy_u256_to_evm_u256(U256::from(genesis_init.config.chain_id));
            info!("[App] Initializing new storage with genesis block. ChainId: {}", chain_id.clone());
            let storage_inner = RedbStorage::new_with_genesis_init(chain_id, genesis_init, storage_path);
            Arc::new(RwLock::new(storage_inner))
        };
        Ok((storage, state_filename))
    }

    fn setup_mempool(&self) -> Result<Arc<RwLock<Mempool>>, Box<dyn std::error::Error>> {
        let mempool = if self.mempool_configured {
            self.mempool.clone().ok_or("Mempool marked as configured but not provided")?
        } else {
            Arc::new(RwLock::new(Mempool::new(U256::ZERO)))
        };
        Ok(mempool)
    }

    fn setup_executor(&self) -> Result<Arc<Executor>, Box<dyn std::error::Error>> {
        let executor = if self.executor_configured {
            self.executor.clone().ok_or("Executor marked as configured but not provided")?
        } else {
            Executor::new()
        };
        Ok(Arc::new(executor))
    }

    fn setup_p2p(&self, args: &Args, p2p_identity: Identity, storage: Arc<RwLock<RedbStorage>>, mempool: Arc<RwLock<Mempool>>, executor: Arc<Executor>) -> Result<(Arc<PeerManager>, Arc<SyncController>), Box<dyn std::error::Error>> {
        let (peer_manager, gossip_rx) = PeerManager::new(
            p2p_identity,
            storage.clone(),
            args.discovery_port,
            args.p2p_port,
            args.ext_ip,
            args.bootnodes.clone(),
        )?;
        let peer_manager = Arc::new(peer_manager);

        let sync_engine = Arc::new(SyncController::new(
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

        Ok((peer_manager, sync_engine))
    }

    fn setup_jwt_secret(&self, args: &Args, data_dir: &PathBuf, storage_name_from_id: &str) -> Result<Option<[u8; 32]>, Box<dyn std::error::Error>> {
        if let Some(ref path) = args.auth_rpc_jwt_path {
            let secret_str = std::fs::read_to_string(path)?;
            let mut secret_str = secret_str.trim();
            if secret_str.starts_with("0x") {
                secret_str = &secret_str[2..];
            }
            let mut secret = [0u8; 32];
            hex::decode_to_slice(secret_str, &mut secret)
                .map_err(|e| format!("Invalid JWT secret at {:?}: {}", path, e))?;
            Ok(Some(secret))
        } else {
            let jwt_path = data_dir.join(format!("jwt_{}.hex", storage_name_from_id));
            if jwt_path.exists() {
                info!("[App] Loading existing JWT secret from {:?}", jwt_path);
                let secret_str = std::fs::read_to_string(&jwt_path)?;
                let mut secret_str = secret_str.trim();
                if secret_str.starts_with("0x") {
                    secret_str = &secret_str[2..];
                }
                let mut secret = [0u8; 32];
                hex::decode_to_slice(secret_str, &mut secret)
                    .map_err(|e| format!("Invalid JWT secret at {:?}: {}", jwt_path, e))?;
                Ok(Some(secret))
            } else {
                info!("[App] Generating new JWT secret for this node...");
                let mut secret_bytes = [0u8; 32];
                getrandom::getrandom(&mut secret_bytes)
                    .map_err(|e| format!("Failed to generate JWT secret: {}", e))?;
                let secret_hex = hex::encode(secret_bytes);
                std::fs::write(&jwt_path, &secret_hex)?;
                info!("[App] New JWT secret saved to {:?}", jwt_path);
                Ok(Some(secret_bytes))
            }
        }
    }

    fn setup_rpc_services(
        &self,
        storage: Arc<RwLock<RedbStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        executor: Arc<Executor>,
        peer_manager: Arc<PeerManager>,
        sync_engine: Arc<SyncController>,
        account_manager: Arc<AccountManager>,
    ) -> Result<(jsonrpsee::RpcModule<()>, jsonrpsee::RpcModule<()>), Box<dyn std::error::Error>> {
        let mut eth_facade = RpcServerFacade::new();
        let mut auth_facade = RpcServerFacade::new();
        let provider = Arc::new(StorageProvider::new(storage.clone(), mempool.clone(), executor.clone()));

        let eth_service = EthService {
            state_storage: provider.clone(),
            mempool: mempool.clone(),
            peer_manager: peer_manager.clone(),
            executor: (*executor).clone(),
            storage: storage.clone(),
            account_manager: account_manager.clone(),
            sync_engine: sync_engine.clone(),
        };

        eth_facade.register_debug(DebugService { mempool: mempool.clone() })?;
        eth_facade.register_eth(eth_service.clone())?;

        auth_facade.register_engine(EngineService::new(
            storage.clone(),
            mempool.clone(),
            (*executor).clone(),
            sync_engine.clone(),
        ))?;
        auth_facade.register_eth(eth_service.clone())?;

        Ok((eth_facade.into_module(), auth_facade.into_module()))
    }

    pub async fn build(self) -> Result<App, Box<dyn std::error::Error>> {
        let args = self.args.clone().ok_or("Args not provided")?;

        self.setup_logging(&args).await;

        let (eth_rpc_addr, auth_rpc_addr, frontend_addr) = self.setup_addresses(&args);
        let data_dir = self.setup_data_dir(&args)?;
        let account_manager = self.setup_account_manager();

        let p2p_identity = Identity::new(
            args.data_dir.as_deref(),
            args.peer_name.as_deref(),
        )?;
        let peer_id = p2p_identity.peer_id();
        info!("[App] P2P Identity generated. PeerId: {}", peer_id);

        let (storage, state_filename) = self.setup_storage(&args, &data_dir, &peer_id).await?;
        let mempool = self.setup_mempool()?;
        let executor = self.setup_executor()?;

        let (peer_manager, sync_engine) = self.setup_p2p(&args, p2p_identity, storage.clone(), mempool.clone(), executor.clone())?;

        let storage_name_from_id = args.peer_name.clone().unwrap_or_else(|| peer_id.clone());
        let auth_rpc_jwt_secret = self.setup_jwt_secret(&args, &data_dir, &storage_name_from_id)?;

        let (eth_module, auth_module) = self.setup_rpc_services(
            storage.clone(),
            mempool.clone(),
            executor.clone(),
            peer_manager.clone(),
            sync_engine.clone(),
            account_manager,
        )?;

        Ok(App {
            eth_rpc_addr,
            auth_rpc_addr,
            frontend_addr,
            eth_rpc_port: args.eth_rpc_port,
            eth_module,
            auth_module,
            swarm: peer_manager,
            gossip_rx: None, // Moved to GossipHandler
            mempool: mempool.clone(),
            storage: storage.clone(),
            executor: executor.clone(),
            data_dir,
            state_filename,
            dev_interval: args.dev,
            auth_rpc_jwt_secret,
            metrics_port: args.metrics_port
        })
    }
}
