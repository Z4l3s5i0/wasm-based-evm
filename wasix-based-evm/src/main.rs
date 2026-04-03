mod ev;
mod executor;
mod storage;
mod rpc;

use crate::executor::Executor;
use crate::storage::InMemoryStorage;
use crate::rpc::MyTransactionService;
use crate::rpc::evm_rpc::transaction_service_server::TransactionServiceServer;
use crate::ev::EvmU256;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    
    let chain_id = EvmU256::from(1);
    let storage_inner = InMemoryStorage::new(chain_id);
    
    // Log pre-funded accounts for clarity
    for (h160, account) in &storage_inner.backend.state {
        println!("Pre-funded account: 0x{:x}, balance: {} wei", h160, account.balance);
    }

    let storage = Arc::new(Mutex::new(storage_inner));
    let executor = Executor::new();

    let transaction_service = MyTransactionService {
        storage,
        executor,
    };

    println!("EVM gRPC Server listening on {}", addr);

    Server::builder()
        .add_service(TransactionServiceServer::new(transaction_service))
        .serve(addr)
        .await?;

    Ok(())
}
