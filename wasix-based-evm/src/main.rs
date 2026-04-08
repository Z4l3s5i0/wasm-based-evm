mod ev;
mod rpc;
mod executor;
mod storage;
mod mempool;
mod network;
mod cli;
mod logging;

use alloy_primitives::U256;
use clap::Parser;
use std::path::PathBuf;

use crate::ev::{alloy_u256_to_evm_u256};
use crate::executor::Executor;
use crate::rpc::MyTransactionService;
use crate::rpc::evm_rpc::transaction_service_server::TransactionServiceServer;
use alloy_genesis::Genesis as AlloyGenesis;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Server;
use crate::storage::genesis::Genesis;
use crate::storage::storage::InMemoryStorage;
use crate::logging::LogLevel;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::Args::parse();

    let level = match args.verbose {
        0 => LogLevel::None,
        1 => LogLevel::Info,
        _ => LogLevel::Debug,
    };
    logging::set_log_level(level);

    let rpc_addr = format!("127.0.0.1:{}", args.rpc_port).parse()?;

    let data_dir = if let Some(dir) = args.data_dir {
        dir
    } else if cfg!(target_os = "wasi") {
        std::env::current_dir()?
    } else {
        std::path::PathBuf::from(std::env!("CARGO_MANIFEST_DIR"))
    };

    let genesis_path = data_dir.join("genesis/genesis.json");
    info!("[Main] Loading genesis from {:?}", genesis_path);
    let genesis_file = std::fs::File::open(genesis_path)?;
    let alloy_genesis: AlloyGenesis = serde_json::from_reader(genesis_file)?;
    let genesis = Genesis::from(alloy_genesis);

    let chain_id = alloy_u256_to_evm_u256(U256::from(genesis.chain_id));
    let storage_inner = InMemoryStorage::new_with_genesis(chain_id, genesis);

    // Log pre-funded accounts for clarity
    for (h160, account) in &storage_inner.backend.state {
        debug!("[Main] Pre-funded account: 0x{:x}, balance: {} wei", h160, account.balance);
    }

    let storage = Arc::new(Mutex::new(storage_inner));
    let executor = Executor::new();

    // Start networking
    let network_config = network::NetworkConfig {
        discv5_addr: format!("0.0.0.0:{}", args.discovery_port).parse()?,
        p2p_addr: format!("0.0.0.0:{}", args.p2p_port).parse()?,
        ext_ip: args.ext_ip,
        bootnodes: args.bootnodes,
        max_peers: args.max_peers,
    };
    let network_handle = network::start_network(network_config, storage.clone()).await?;

    let transaction_service = MyTransactionService {
        storage,
        executor,
        pending_payloads: Arc::new(Mutex::new(std::collections::HashMap::new())),
        network_handle: Some(network_handle),
    };

    info!("[Main] EVM gRPC Server listening on {}", rpc_addr);

    Server::builder()
        .add_service(TransactionServiceServer::new(transaction_service))
        .serve(rpc_addr)
        .await?;

    Ok(())
}
