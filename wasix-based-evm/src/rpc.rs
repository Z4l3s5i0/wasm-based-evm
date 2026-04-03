use crate::executor::Executor;
use crate::storage::{InMemoryStorage, Transaction, Block};
use crate::ev::address_to_h160;
use alloy_primitives::{Address, U256};
use tonic::{Request, Response, Status};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod evm_rpc {
    tonic::include_proto!("evm_rpc");
}

use evm_rpc::transaction_service_server::TransactionService;
use evm_rpc::{TransactionRequest, TransactionResponse};

pub struct MyTransactionService {
    pub storage: Arc<Mutex<InMemoryStorage>>,
    pub executor: Executor,
}

#[tonic::async_trait]
impl TransactionService for MyTransactionService {
    async fn execute_transaction(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();

        let from_addr: Address = req.from.parse().map_err(|_| Status::invalid_argument("Invalid from address"))?;
        let to_addr: Option<Address> = if req.to.is_empty() {
            None
        } else {
            Some(req.to.parse().map_err(|_| Status::invalid_argument("Invalid to address"))?)
        };

        let tx = Transaction::builder(from_addr)
            .nonce(req.nonce)
            .to(to_addr)
            .value(U256::from(req.value))
            .data(req.data)
            .gas_limit(req.gas_limit)
            .gas_price(U256::from(req.gas_price))
            .build();

        let block = Block::builder(1)
            .timestamp(123456789)
            .add_transaction(tx.hash)
            .build();


        let mut storage = self.storage.lock().await;
        match self.executor.execute(&mut *storage, tx.clone(), block) {
            Ok(_) => {
                println!("Transaction executed successfully: {:?}", tx.hash);
                let from_h160 = address_to_h160(from_addr);
                let sender_balance = storage.backend.state.get(&from_h160).map(|a| a.balance).unwrap_or_default();
                println!("new balance for sender {:?}", sender_balance);

                if let Some(to_addr) = to_addr {
                    let to_h160 = address_to_h160(to_addr);
                    let receiver_balance = storage.backend.state.get(&to_h160).map(|a| a.balance).unwrap_or_default();
                    println!("new balance for receiver {:?}", receiver_balance);
                }

                Ok(Response::new(TransactionResponse {
                    success: true,
                    message: "Transaction executed successfully".to_string(),
                    tx_hash: format!("{:?}", tx.hash),
                }))
            }
            Err(e) => {
                println!("Transaction failed: {:?}, error: {}", tx.hash, e);
                Ok(Response::new(TransactionResponse {
                    success: false,
                    message: format!("Transaction failed: {}", e),
                    tx_hash: format!("{:?}", tx.hash),
                }))
            }
        }
    }
}
