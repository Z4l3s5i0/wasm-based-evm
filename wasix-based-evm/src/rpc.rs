use crate::executor::Executor;
use crate::storage::{InMemoryStorage, Transaction, Block};
use crate::ev::h160_to_address;
use alloy_primitives::{Address, U256, B256};
use tonic::{Request, Response, Status};
use std::sync::Arc;
use tokio::sync::Mutex;
use evm::standard::TransactValueCallCreate;

pub mod evm_rpc {
    tonic::include_proto!("evm_rpc");
}

use evm_rpc::transaction_service_server::TransactionService;
use evm_rpc::{
    TransactionRequest, TransactionResponse, Empty, AccountsResponse, BlockNumberResponse,
    GasPriceResponse, GetBalanceRequest, BalanceResponse, GetBlockByNumberRequest,
    GetBlockByHashRequest, BlockResponse, GetBlockTransactionCountByHashRequest,
    GetBlockTransactionCountByNumberRequest, TransactionCountResponse,
    GetTransactionByHashRequest, TransactionInfoResponse, TransactionReceiptResponse,
    GetCodeRequest, CodeResponse, RootsResponse
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

        let mut storage = self.storage.lock().await;
        let latest_block = storage.get_latest_block().cloned().expect("Genesis block should exist");
        let next_number = latest_block.body.execution_payload.block_number + 1;
        
        let val_u256 = U256::from_str_radix(&req.value, 10).or_else(|_| {
            // Try hex if decimal fails
            U256::from_str_radix(req.value.trim_start_matches("0x"), 16)
        }).map_err(|_| Status::invalid_argument("Invalid value"))?;

        let tx = Transaction::builder(from_addr)
            .nonce(req.nonce)
            .to(to_addr)
            .value(val_u256)
            .data(req.data)
            .gas_limit(req.gas_limit)
            .gas_price(U256::from(req.gas_price))
            .build();

        println!("DEBUG: Executing tx: from={:?}, to={:?}, value={}", from_addr, to_addr, val_u256);

        let block = Block::builder(next_number)
            .parent_hash(latest_block.body.execution_payload.block_hash)
            .timestamp(latest_block.body.execution_payload.timestamp + 12) // Simple block time increment
            .state_root(B256::ZERO)
            .add_transaction(tx.clone())
            .build();

        match self.executor.execute(&mut *storage, tx.clone(), block) {
            Ok(val) => {
                println!("Transaction executed successfully: {:?}", tx.hash);
                let sender_balance = storage.get_balance(from_addr);
                println!("new balance for sender {}", sender_balance);

                if let Some(to_addr) = to_addr {
                    let receiver_balance = storage.get_balance(to_addr);
                    println!("new balance for receiver {}", receiver_balance);
                }

                let (contract_address, return_data) = match val.call_create {
                    TransactValueCallCreate::Call { retval, .. } => {
                        (String::new(), retval)
                    }
                    TransactValueCallCreate::Create { address, .. } => {
                        let addr = h160_to_address(address);
                        storage.set_contract_code(addr, tx.data.clone()); // Optional: if you want to explicitly track it in storage.contracts
                        (format!("{:?}", addr), Vec::new())
                    }
                };

                Ok(Response::new(TransactionResponse {
                    success: true,
                    message: "Transaction executed successfully".to_string(),
                    tx_hash: format!("{:?}", tx.hash),
                    contract_address,
                    return_data,
                }))
            }
            Err(e) => {
                println!("Transaction failed: {:?}, error: {}", tx.hash, e);
                Ok(Response::new(TransactionResponse {
                    success: false,
                    message: format!("Transaction failed: {}", e),
                    tx_hash: format!("{:?}", tx.hash),
                    contract_address: String::new(),
                    return_data: Vec::new(),
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
            number: block.body.execution_payload.block_number,
            hash: format!("{:?}", block.body.execution_payload.block_hash),
            parent_hash: format!("{:?}", block.body.execution_payload.parent_hash),
            timestamp: block.body.execution_payload.timestamp,
            transactions: block.body.execution_payload.transactions.iter().map(|t| format!("{:?}", t.hash)).collect(),
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
            number: block.body.execution_payload.block_number,
            hash: format!("{:?}", block.body.execution_payload.block_hash),
            parent_hash: format!("{:?}", block.body.execution_payload.parent_hash),
            timestamp: block.body.execution_payload.timestamp,
            transactions: block.body.execution_payload.transactions.iter().map(|t| format!("{:?}", t.hash)).collect(),
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
            count: block.body.execution_payload.transactions.len() as u64,
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
            count: block.body.execution_payload.transactions.len() as u64,
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

        let block = storage.get_block_by_transaction_hash(hash);

        Ok(Response::new(TransactionInfoResponse {
            hash: format!("{:?}", tx.hash),
            nonce: tx.nonce,
            from: format!("{:?}", tx.from),
            to: tx.to.map(|a| format!("{:?}", a)).unwrap_or_default(),
            value: tx.value.to_string(),
            data: tx.data.clone(),
            gas_limit: tx.gas_limit,
            gas_price: tx.gas_price.to_string().parse().unwrap_or(0),
            block_number: block.map(|b| b.body.execution_payload.block_number).unwrap_or(0),
            block_hash: block.map(|b| format!("{:?}", b.body.execution_payload.block_hash)).unwrap_or_default(),
        }))
    }

    async fn eth_get_transaction_receipt(
        &self,
        request: Request<GetTransactionByHashRequest>,
    ) -> Result<Response<TransactionReceiptResponse>, Status> {
        use crate::rpc::evm_rpc::{TransactionReceiptResponse, LogEntry};
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let storage = self.storage.lock().await;
        
        let tx = storage.get_transaction_by_hash(hash)
            .ok_or_else(|| Status::not_found("Transaction not found"))?;
        let receipt = storage.get_receipt_by_tx_hash(hash)
            .ok_or_else(|| Status::not_found("Receipt not found"))?;
        let block = storage.get_block_by_transaction_hash(hash)
            .ok_or_else(|| Status::not_found("Block for transaction not found"))?;

        let tx_index = block.body.execution_payload.transactions.iter()
            .position(|t| t.hash == hash)
            .unwrap_or(0) as u64;

        let logs = receipt.logs.iter().enumerate().map(|(i, log)| {
            LogEntry {
                address: format!("{:?}", log.address),
                topics: log.topics.iter().map(|t| format!("{:?}", t)).collect(),
                data: log.data.clone(),
                block_number: block.body.execution_payload.block_number,
                block_hash: format!("{:?}", block.body.execution_payload.block_hash),
                transaction_hash: format!("{:?}", hash),
                transaction_index: tx_index,
                log_index: i as u64,
            }
        }).collect();

        Ok(Response::new(TransactionReceiptResponse {
            transaction_hash: format!("{:?}", hash),
            transaction_index: tx_index,
            block_hash: format!("{:?}", block.body.execution_payload.block_hash),
            block_number: block.body.execution_payload.block_number,
            from: format!("{:?}", tx.from),
            to: tx.to.map(|a| format!("{:?}", a)).unwrap_or_default(),
            cumulative_gas_used: receipt.cumulative_gas_used,
            gas_used: receipt.cumulative_gas_used, // Simplified
            contract_address: String::new(), // TODO: implement if needed
            logs,
            logs_bloom: format!("{:?}", receipt.logs_bloom),
            status: if receipt.success { 1 } else { 0 },
        }))
    }

    async fn eth_send_transaction(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        // For now, eth_sendTransaction is the same as execute_transaction
        self.execute_transaction(request).await
    }

    async fn eth_call(
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

        let storage = self.storage.lock().await;
        let latest_block = storage.get_latest_block().cloned().expect("Genesis block should exist");
        
        let val_u256 = U256::from_str_radix(&req.value, 10).or_else(|_| {
            // Try hex if decimal fails
            U256::from_str_radix(req.value.trim_start_matches("0x"), 16)
        }).map_err(|_| Status::invalid_argument("Invalid value"))?;

        let tx = Transaction::builder(from_addr)
            .nonce(req.nonce)
            .to(to_addr)
            .value(val_u256)
            .data(req.data)
            .gas_limit(req.gas_limit)
            .gas_price(U256::from(req.gas_price))
            .build();

        println!("DEBUG: Calling eth_call: from={:?}, to={:?}, value={}", from_addr, to_addr, val_u256);

        // For eth_call, we use the latest block state without incrementing the block number
        let block = latest_block;

        match self.executor.call(&*storage, tx.clone(), block) {
            Ok(val) => {
                let (_, return_data) = match val.call_create {
                    TransactValueCallCreate::Call { retval, .. } => {
                        (String::new(), retval)
                    }
                    TransactValueCallCreate::Create { address, .. } => {
                        let addr = h160_to_address(address);
                        (format!("{:?}", addr), Vec::new())
                    }
                };

                Ok(Response::new(TransactionResponse {
                    success: true,
                    message: "Call executed successfully".to_string(),
                    tx_hash: String::new(),
                    contract_address: String::new(),
                    return_data,
                }))
            }
            Err(e) => {
                println!("Call failed: error: {}", e);
                Ok(Response::new(TransactionResponse {
                    success: false,
                    message: format!("Call failed: {}", e),
                    tx_hash: String::new(),
                    contract_address: String::new(),
                    return_data: Vec::new(),
                }))
            }
        }
    }

    async fn eth_get_code(
        &self,
        request: Request<GetCodeRequest>,
    ) -> Result<Response<CodeResponse>, Status> {
        let req = request.into_inner();
        let address: Address = req.address.parse().map_err(|_| Status::invalid_argument("Invalid address"))?;
        let storage = self.storage.lock().await;
        let code = storage.get_code(address);
        
        Ok(Response::new(CodeResponse {
            code,
        }))
    }

    async fn eth_get_roots(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<RootsResponse>, Status> {
        let storage = self.storage.lock().await;
        let block = storage.get_latest_block().expect("Genesis block should exist");
        
        let calculated_state_root = storage.calculate_state_root();
        let calculated_tx_root = InMemoryStorage::calculate_transactions_root(&block.body.execution_payload.transactions);
        let calculated_receipts_root = InMemoryStorage::calculate_receipts_root(&storage.get_block_receipts(block.body.execution_payload.block_hash));
        let calculated_withdrawals_root = InMemoryStorage::calculate_withdrawals_root(&block.body.execution_payload.withdrawals);

        Ok(Response::new(RootsResponse {
            state_root: format!("{:?} (calc: {:?})", block.body.execution_payload.state_root, calculated_state_root),
            transactions_root: format!("{:?} (calc: {:?})", block.body.execution_payload.transactions_root, calculated_tx_root),
            receipts_root: format!("{:?} (calc: {:?})", block.body.execution_payload.receipts_root, calculated_receipts_root),
            withdrawals_root: format!("{:?} (calc: {:?})", block.body.execution_payload.withdrawals_root, calculated_withdrawals_root),
        }))
    }
}
