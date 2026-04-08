use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// TCP port for RLPx/Tentacle
    #[arg(long, default_value_t = 9001)]
    pub p2p_port: u16,

    /// UDP port for Discv5
    #[arg(long, default_value_t = 9000)]
    pub discovery_port: u16,

    /// gRPC/JSON-RPC port
    #[arg(long, default_value_t = 50051)]
    pub rpc_port: u16,

    /// Comma-separated list of ENRs for bootstrapping
    #[arg(long, value_delimiter = ',')]
    pub bootnodes: Vec<String>,

    /// Maximum number of concurrent P2P connections
    #[arg(long, default_value_t = 50)]
    pub max_peers: usize,
    
    /// External IP to report in ENR (optional)
    #[arg(long)]
    pub ext_ip: Option<std::net::IpAddr>,

    /// Path for persistent storage
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Chain name (mainnet, sepolia, devnet)
    #[arg(long, default_value = "devnet")]
    pub chain: String,

    /// Verbosity level (0: none, 1: info, 2: debug)
    #[arg(long, default_value_t = 1)]
    pub verbose: u8,
}
