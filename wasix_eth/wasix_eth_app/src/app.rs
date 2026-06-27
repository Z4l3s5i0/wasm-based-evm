use crate::cli::Args;
use crate::jwt::HeaderInjectorLayer;
use crate::jwt::JwtAuthLayer;
use crate::node::Node;
use jsonrpsee::server::middleware::rpc::RpcServiceBuilder;
use jsonrpsee::server::Server;
use serde_json::from_reader;
use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use wasix_eth_rpc::AdminService;
use wasix_eth_rpc::DebugService;
use wasix_eth_rpc::EngineService;
use wasix_eth_rpc::EthService;
use wasix_eth_rpc::RpcServerFacade;
use wasix_eth_types::genesis::GenesisConfiguration;
use wasix_eth_utils::debug;
use wasix_eth_utils::error;
use wasix_eth_utils::info;
use wasix_eth_utils::logging;
use wasix_eth_utils::logging::LogLevel;

pub struct App {
    args: Args,
    node: Option<Node>,
    eth_rpc_server: Option<Server>,
    auth_rpc_server: Option<Server<tower::layer::util::Stack<HeaderInjectorLayer, tower::layer::util::Identity>, tower::layer::util::Stack<JwtAuthLayer, tower::layer::util::Identity>>>,
    eth_module: Option<jsonrpsee::RpcModule<()>>,
    auth_module: Option<jsonrpsee::RpcModule<()>>,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::default()
    }
    pub fn node(&self) -> Option<&Node> {
        self.node.as_ref()
    }

    pub async fn run(mut self) -> Result<(), Box<dyn Error>> {
        info!("Setup complete...");
        info!("Starting servers...");

        // Spawn metrics collection task
        let data_dir = self.args.common.data_dir.clone();
        tokio::spawn(async move {
            info!("[App] Starting metrics collection task");
            loop {
                // Update Node Uptime
                wasix_eth_utils::metrics::NODE_UPTIME.inc_by(15.0);

                // Update Storage DB Size
                if let Ok(metadata) = std::fs::metadata(&data_dir) {
                    if metadata.is_dir() {
                        // Simple recursive size check if possible, or just the dir size
                        // In WASI/Wasix this might be limited, but let's try a basic estimate
                        if let Ok(entries) = std::fs::read_dir(&data_dir) {
                            let mut total_size = 0u64;
                            for entry in entries.flatten() {
                                if let Ok(meta) = entry.metadata() {
                                    total_size += meta.len();
                                }
                            }
                            wasix_eth_utils::metrics::STORAGE_DB_SIZE.set(total_size as f64);
                        }
                    }
                }

                // WASM Memory Usage (if running in WASM environment)
                // In Wasix/WASI, we can check current memory size
                #[cfg(target_family = "wasm")]
                {
                    let mem = core::arch::wasm32::memory_size(0);
                    wasix_eth_utils::metrics::WASM_MEMORY_USAGE.set((mem * 64 * 1024) as f64);
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
            }
        });

        let node = self.node.as_mut().ok_or("Node not initialized")?;
        node.start(&self.args).await;

        let eth_module = self.eth_module.ok_or("Eth module not initialized")?;
        let auth_module = self.auth_module.ok_or("Auth module not initialized")?;
        let eth_rpc_server = self.eth_rpc_server.ok_or("Eth RPC server not initialized")?;
        let auth_rpc_server = self.auth_rpc_server.ok_or("Auth RPC server not initialized")?;

        let eth_rpc_handle = eth_rpc_server.start(eth_module);
        let auth_rpc_handle = auth_rpc_server.start(auth_module);

        info!("Servers are running, waiting for shutdown signal...");

        // Keep the application running by waiting for the RPC servers to stop or a signal
        tokio::select! {
            _ = eth_rpc_handle.stopped() => {
                info!("Eth RPC server stopped");
            }
            _ = auth_rpc_handle.stopped() => {
                info!("Auth RPC server stopped");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutdown signal received");
            }
        }

        Ok(())
    }

    pub async fn init(self) -> Result<(), Box<dyn Error>> {
        info!("[App] Genesis initialized. Exiting.");
        Ok(())
    }

    pub async fn import(self) -> Result<(), Box<dyn Error>> {
        info!("[App] Block import completed. Exiting.");
        Ok(())
    }
}

#[derive(Default)]
pub struct AppBuilder {
    args: Option<Args>,
}

impl AppBuilder {
    pub fn with_config(mut self, args: Args) -> Self {
        self.args = Some(args);
        self
    }
    pub fn setup_genesis(&self, genesis_path: PathBuf) -> Result<GenesisConfiguration, Box<dyn Error>> {
        let genesis_file = std::fs::File::open(genesis_path)?;
        let genesis_config: GenesisConfiguration = from_reader(genesis_file)?;
        Ok(genesis_config)
    }
    async fn setup_logging(&self, args: &Args) {
        let level = match args.common.verbose {
            0 => LogLevel::None,
            1 => LogLevel::Info,
            _ => LogLevel::Debug,
        };
        logging::set_log_level(level);
        wasix_eth_utils::metrics::init_metrics();
    }

    pub fn setup_jwt_secret(&self, args: &Args, data_dir: &PathBuf, storage_name_from_id: &str) -> Result<Option<[u8; 32]>, Box<dyn std::error::Error>> {
        if let Some(ref path) = args.common.auth_rpc_jwt_path {
            info!("[App] Loading JWT secret from {:?}", path);
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

    async fn setup_rpc_services(
        &self,
        node: &Node,
        ext_ip: Option<String>,
    ) -> Result<(jsonrpsee::RpcModule<()>, jsonrpsee::RpcModule<()>), Box<dyn Error>> {
        info!("[App] Setting up RPC services");
        let mut eth_facade = RpcServerFacade::new();
        let mut auth_facade = RpcServerFacade::new();

        let eth_service = EthService {
            engine: node.rpc_engine.clone(),
        };
        let debug_service = DebugService::new(node.mempool.clone(), node.rpc_engine.clone());
        let engine_service = EngineService {
            engine: node.engine.clone(),
        };
        let peer_manager = node.peer_manager.clone().expect("PeerManager is required for RPC");
        let admin_service = AdminService::new(peer_manager, node.read_provider.clone(), ext_ip);

        eth_facade.register_debug(debug_service)?;
        eth_facade.register_eth(eth_service.clone())?;
        eth_facade.register_admin(admin_service)?;

        auth_facade.register_engine(engine_service)?;
        auth_facade.register_eth(eth_service)?;
        info!("[App] RPC services set up");
        Ok((eth_facade.into_module(), auth_facade.into_module()))
    }

    async fn setup_core_server(
        &self,
        eth_server_address: SocketAddr,
        auth_server_address: SocketAddr,
        auth_secret: Option<[u8; 32]>,
    ) -> Result<(Server, Server<tower::layer::util::Stack<HeaderInjectorLayer, tower::layer::util::Identity>, tower::layer::util::Stack<JwtAuthLayer, tower::layer::util::Identity>>), Box<dyn Error>> {
        info!("[App] Setting up RPC servers");

        let eth_server = Server::builder()
            .build(eth_server_address)
            .await?;

        let rpc_middleware = RpcServiceBuilder::new();
        let http_middleware = tower::ServiceBuilder::new()
            .layer(HeaderInjectorLayer);
        if let Some(secret) = auth_secret {
            let auth_server = Server::builder()
                .set_rpc_middleware(rpc_middleware.layer(JwtAuthLayer::new(secret)))
                .set_http_middleware(http_middleware)
                .build(auth_server_address)
                .await?;

            info!("[App] RPC servers started");
            Ok((eth_server, auth_server))
        } else {
            error!("[App] JWT authentication key missing: Auth RPC requires JWT authentication");
            Err("JWT authentication key missing for Auth RPC".into())
        }
    }
    pub fn setup_addresses(&self, args: &Args) -> (SocketAddr, SocketAddr) {
        info!("[App] Setting up RPC servers addresses");
        let bind_ip = "0.0.0.0".parse().unwrap();
        let eth_rpc_addr = SocketAddr::new(bind_ip, args.common.eth_rpc_port);
        let auth_rpc_addr = SocketAddr::new(bind_ip, args.common.auth_rpc_port);
        info!("[App] Addresses: Eth RPC: {}, Auth RPC: {}", eth_rpc_addr, auth_rpc_addr);
        (eth_rpc_addr, auth_rpc_addr)
    }

    pub async fn build_init(self) -> Result<App, Box<dyn Error>> {
        debug!("[App] Building app for init");
        let args = self.args.clone().ok_or("Config not provided")?;
        self.setup_logging(&args).await;

        let data_dir = args.common.data_dir.clone().unwrap_or_else(|| PathBuf::from("data"));
        std::fs::create_dir_all(&data_dir)?;

        let genesis_path = args.common.genesis_path.as_ref().ok_or("Genesis path not provided. 'init' command requires --genesis-path")?;
        let genesis_configuration = self.setup_genesis(genesis_path.clone())?;

        let storage_name = args.common.peer_name.clone().unwrap_or_else(|| "default".to_string());
        let _jwt_secret = self.setup_jwt_secret(&args, &data_dir, &storage_name)?
            .unwrap_or([0u8; 32]);

        // Initialize only storage
        Node::new_storage_only(&args, genesis_configuration).await?;

        Ok(App {
            args,
            node: None,
            eth_rpc_server: None,
            auth_rpc_server: None,
            eth_module: None,
            auth_module: None,
        })
    }

    pub async fn build_import(self) -> Result<App, Box<dyn Error>> {
        debug!("[App] Building app for import");
        let args = self.args.clone().ok_or("Config not provided")?;
        self.setup_logging(&args).await;

        let data_dir = args.common.data_dir.clone().unwrap_or_else(|| PathBuf::from("data"));
        std::fs::create_dir_all(&data_dir)?;

        let storage_name = args.common.peer_name.clone().unwrap_or_else(|| "default".to_string());
        let _jwt_secret = self.setup_jwt_secret(&args, &data_dir, &storage_name)?
            .unwrap_or([0u8; 32]);

        // Initialize Node without genesis - this will fail if genesis hasn't been initialized
        let node = Node::new_no_init(&args).await
            .map_err(|e| format!("Import failed: Genesis must be initialized before importing blocks. Error: {}", e))?;

        // Hive block import
        node.import_blocks(args.common.import_chain.clone(), args.common.import_blocks.clone()).await?;

        Ok(App {
            args,
            node: Some(node),
            eth_rpc_server: None,
            auth_rpc_server: None,
            eth_module: None,
            auth_module: None,
        })
    }

    pub async fn build(self) -> Result<App, Box<dyn Error>> {
        debug!("[App] Building app");
        let args = self.args.clone().ok_or("Config not provided")?;
        self.setup_logging(&args).await;

        let data_dir = args.common.data_dir.clone().unwrap_or_else(|| PathBuf::from("data"));
        std::fs::create_dir_all(&data_dir)?;
        
        let genesis_configuration = if let Some(genesis_path) = &args.common.genesis_path {
            Some(self.setup_genesis(genesis_path.clone())?)
        } else {
            None
        };

        let storage_name = args.common.peer_name.clone().unwrap_or_else(|| "default".to_string());
        let jwt_secret = self.setup_jwt_secret(&args, &data_dir, &storage_name)?
            .unwrap_or([0u8; 32]);

        // Initialize Node (Coordinator)
        let node = Node::new(&args, genesis_configuration).await?;

        // Setup RPC services
        let (eth_rpc_addr, auth_rpc_addr) = self.setup_addresses(&args);

        let ext_ip_str = args.common.ext_ip.map(|ip| ip.to_string());
        let (eth_module, auth_module) = self.setup_rpc_services(&node, ext_ip_str).await?;
        let (eth_rpc_server, auth_rpc_server) = self.setup_core_server(eth_rpc_addr, auth_rpc_addr, Some(jwt_secret)).await?;

        debug!("[App] App built with all services");
        Ok(App {
            args,
            node: Some(node),
            eth_rpc_server: Some(eth_rpc_server),
            auth_rpc_server: Some(auth_rpc_server),
            eth_module: Some(eth_module),
            auth_module: Some(auth_module),
        })
    }
}
