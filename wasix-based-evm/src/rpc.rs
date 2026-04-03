use crate::executor::Executor;
use crate::storage::{InMemoryStorage, Transaction, Block};
use crate::ev::address_to_h160;
use alloy_primitives::{Address, U256, B256};
use tonic::{Request, Response, Status};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod evm_rpc {
    tonic::include_proto!("evm_rpc");
}

use evm_rpc::transaction_service_server::TransactionService;
use evm_rpc::{
    TransactionRequest, TransactionResponse, Empty, AccountsResponse, BlockNumberResponse,
    GasPriceResponse, GetBalanceRequest, BalanceResponse, GetBlockByNumberRequest,
    GetBlockByHashRequest, BlockResponse, GetBlockTransactionCountByHashRequest,
    GetBlockTransactionCountByNumberRequest, TransactionCountResponse,
    GetTransactionByHashRequest, TransactionInfoResponse
};

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

    async fn eth_accounts(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<AccountsResponse>, Status> {
        let storage = self.storage.lock().await;
        let accounts = storage.get_accounts();
        Ok(Response::new(AccountsResponse {
            addresses: accounts.into_iter().map(|a| format!("{:?}", a)).collect(),
        }))
    }

    async fn eth_block_number(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<BlockNumberResponse>, Status> {
        let storage = self.storage.lock().await;
        let number = storage.get_latest_block_number();
        Ok(Response::new(BlockNumberResponse { number }))
    }

    async fn eth_gas_price(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GasPriceResponse>, Status> {
        // Return a fixed gas price for now
        Ok(Response::new(GasPriceResponse { price: 20000000000 }))
    }

    async fn eth_get_balance(
        &self,
        request: Request<GetBalanceRequest>,
    ) -> Result<Response<BalanceResponse>, Status> {
        let req = request.into_inner();
        let address: Address = req.address.parse().map_err(|_| Status::invalid_argument("Invalid address"))?;
        let storage = self.storage.lock().await;
        let balance = storage.get_balance(address);
        Ok(Response::new(BalanceResponse {
            balance: balance.to_string(),
        }))
    }

    async fn eth_get_block_by_number(
        &self,
        request: Request<GetBlockByNumberRequest>,
    ) -> Result<Response<BlockResponse>, Status> {
        let req = request.into_inner();
        let storage = self.storage.lock().await;
        let block = storage.get_block_by_number(req.number)
            .ok_or_else(|| Status::not_found("Block not found"))?;

        Ok(Response::new(BlockResponse {
            number: block.number,
            hash: format!("{:?}", block.hash),
            parent_hash: format!("{:?}", block.parent_hash),
            timestamp: block.timestamp,
            transactions: block.transactions.iter().map(|t| format!("{:?}", t)).collect(),
        }))
    }

    async fn eth_get_block_by_hash(
        &self,
        request: Request<GetBlockByHashRequest>,
    ) -> Result<Response<BlockResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let storage = self.storage.lock().await;
        let block = storage.get_block_by_hash(hash)
            .ok_or_else(|| Status::not_found("Block not found"))?;

        Ok(Response::new(BlockResponse {
            number: block.number,
            hash: format!("{:?}", block.hash),
            parent_hash: format!("{:?}", block.parent_hash),
            timestamp: block.timestamp,
            transactions: block.transactions.iter().map(|t| format!("{:?}", t)).collect(),
        }))
    }

    async fn eth_get_block_transaction_count_by_hash(
        &self,
        request: Request<GetBlockTransactionCountByHashRequest>,
    ) -> Result<Response<TransactionCountResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let storage = self.storage.lock().await;
        let block = storage.get_block_by_hash(hash)
            .ok_or_else(|| Status::not_found("Block not found"))?;

        Ok(Response::new(TransactionCountResponse {
            count: block.transactions.len() as u64,
        }))
    }

    async fn eth_get_block_transaction_count_by_number(
        &self,
        request: Request<GetBlockTransactionCountByNumberRequest>,
    ) -> Result<Response<TransactionCountResponse>, Status> {
        let req = request.into_inner();
        let storage = self.storage.lock().await;
        let block = storage.get_block_by_number(req.number)
            .ok_or_else(|| Status::not_found("Block not found"))?;

        Ok(Response::new(TransactionCountResponse {
            count: block.transactions.len() as u64,
        }))
    }

    async fn eth_get_transaction_by_hash(
        &self,
        request: Request<GetTransactionByHashRequest>,
    ) -> Result<Response<TransactionInfoResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let storage = self.storage.lock().await;
        let tx = storage.get_transaction_by_hash(hash)
            .ok_or_else(|| Status::not_found("Transaction not found"))?;

        Ok(Response::new(TransactionInfoResponse {
            hash: format!("{:?}", tx.hash),
            nonce: tx.nonce,
            from: format!("{:?}", tx.from),
            to: tx.to.map(|a| format!("{:?}", a)).unwrap_or_default(),
            value: tx.value.to_string(),
            data: tx.data.clone(),
            gas_limit: tx.gas_limit,
            gas_price: tx.gas_price.to_string().parse().unwrap_or(0), // Simplified
            block_number: 0, // Not stored in Transaction yet
            block_hash: String::new(), // Not stored in Transaction yet
        }))
    }

    async fn eth_send_transaction(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        // For now, eth_sendTransaction is the same as execute_transaction
        self.execute_transaction(request).await
    }
}
