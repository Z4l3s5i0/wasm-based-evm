use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// TCP port for discovery
    #[arg(long, default_value_t = 9001)]
    pub discovery_port: u16,

    /// TCP port for P2P (gossip, blocks)
    #[arg(long, default_value_t = 9002)]
    pub p2p_port: u16,

    /// TCP port for Eth JSON-RPC
    #[arg(long, default_value_t = 8545)]
    pub eth_rpc_port: u16,

    /// TCP port for Auth Engine JSON-RPC
    #[arg(long, default_value_t = 8551)]
    pub auth_rpc_port: u16,

    /// TCP port for Frontend (logs)
    #[arg(long, default_value_t = 3000)]
    pub frontend_port: u16,

    /// Comma-separated list of Multiaddrs for bootstrapping (must include /p2p/PeerId)
    #[arg(long, value_delimiter = ',')]
    pub bootnodes: Vec<String>,

    /// Maximum number of concurrent P2P connections
    #[arg(long, default_value_t = 50)]
    pub max_peers: usize,
    
    /// External IP (optional)
    #[arg(long)]
    pub ext_ip: Option<std::net::IpAddr>,

    /// Path for persistent storage
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Path to the genesis JSON file
    #[arg(long)]
    pub genesis_path: Option<PathBuf>,

    /// Chain name (mainnet, sepolia, devnet)
    #[arg(long, default_value = "devnet")]
    pub chain: String,

    /// Path to the JWT secret for the Auth Engine JSON-RPC
    #[arg(long)]
    pub auth_rpc_jwt_path: Option<PathBuf>,

    /// Verbosity level (0: none, 1: info, 2: debug)
    #[arg(long, default_value_t = 1)]
    pub verbose: u8,

    /// Interval in seconds for automatic block production in dev mode
    #[arg(long, default_missing_value = "12", num_args = 0..=1)]
    pub dev: Option<u64>,

    /// Descriptive name for the node, used in storage filenames
    #[arg(long)]
    pub peer_name: Option<String>,

    /// Descriptive name for the node, used in storage filenames
    #[arg(long, default_value_t = 9050)]
    pub metrics_port: u16,
}
