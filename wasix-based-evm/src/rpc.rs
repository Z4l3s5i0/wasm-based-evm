use crate::executor::Executor;
use crate::storage::{InMemoryStorage, Transaction, Block};
use crate::ev::address_to_h160;
use alloy_primitives::{Address, B256, U256};
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

        let tx = Transaction {
            hash: B256::random(), // For demo, generate random hash
            nonce: req.nonce,
            from: from_addr,
            to: to_addr,
            value: U256::from(req.value),
            data: req.data,
            gas_limit: req.gas_limit,
            gas_price: U256::from(req.gas_price),
        };

        let block = Block {
            number: 1, // Simplified for demo
            hash: B256::random(),
            parent_hash: B256::ZERO,
            timestamp: 123456789,
            transactions: vec![tx.hash],
        };

        // Pre-fund the sender for demo purposes if balance is 0
        {
            let mut storage = self.storage.lock().await;
            let from_h160 = address_to_h160(from_addr);
            if !storage.backend.state.contains_key(&from_h160) {
                storage.set_balance(from_addr, U256::from(1000000000000000000u64)); // 1 ETH
            }
        }

        let mut storage = self.storage.lock().await;
        match self.executor.execute(&mut *storage, tx.clone(), block) {
            Ok(_) => {
                Ok(Response::new(TransactionResponse {
                    success: true,
                    message: "Transaction executed successfully".to_string(),
                    tx_hash: format!("{:?}", tx.hash),
                }))
            }
            Err(e) => {
                Ok(Response::new(TransactionResponse {
                    success: false,
                    message: format!("Transaction failed: {}", e),
                    tx_hash: format!("{:?}", tx.hash),
                }))
            }
        }
    }
}
