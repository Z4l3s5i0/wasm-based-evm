mod ev;
mod rpc;
mod executor;
mod storage;
mod mempool;
mod network;
use alloy_primitives::U256;

use crate::ev::{alloy_u256_to_evm_u256};
use crate::executor::Executor;
use crate::storage::{InMemoryStorage, Genesis};
use crate::rpc::MyTransactionService;
use crate::rpc::evm_rpc::transaction_service_server::TransactionServiceServer;
use alloy_genesis::Genesis as AlloyGenesis;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;

    let data_dir = if cfg!(target_os = "wasi") {
        std::env::current_dir()?
    } else {
        std::path::PathBuf::from(std::env!("CARGO_MANIFEST_DIR"))
    };

    let genesis_path = data_dir.join("genesis/genesis.json");
    println!("Loading genesis from {:?}", genesis_path);
    let genesis_file = std::fs::File::open(genesis_path)?;
    let alloy_genesis: AlloyGenesis = serde_json::from_reader(genesis_file)?;
    let genesis = Genesis::from(alloy_genesis);

    let chain_id = alloy_u256_to_evm_u256(U256::from(genesis.chain_id));
    let storage_inner = InMemoryStorage::new_with_genesis(chain_id, genesis);

    // Log pre-funded accounts for clarity
    for (h160, account) in &storage_inner.backend.state {
        println!("Pre-funded account: 0x{:x}, balance: {} wei", h160, account.balance);
    }

    let storage = Arc::new(Mutex::new(storage_inner));
    let executor = Executor::new();

    // Start networking
    let network_config = network::NetworkConfig {
        discv5_addr: "0.0.0.0:9000".parse()?,
        p2p_addr: "0.0.0.0:9001".parse()?,
        bootnodes: vec![], // Add default bootnodes here if any
    };
    network::start_network(network_config, storage.clone()).await?;

    let transaction_service = MyTransactionService {
        storage,
        executor,
        pending_payloads: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };



    println!("EVM gRPC Server listening on {}", addr);

    Server::builder()
        .add_service(TransactionServiceServer::new(transaction_service))
        .serve(addr)
        .await?;

    Ok(())
}
