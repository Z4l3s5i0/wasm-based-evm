use crate::rpc::{MyTransactionService, TransactionRequest, TransactionResponse, ProposeBlockRequest, ProposeBlockResponse, ExecutionPayload, PayloadStatus, ForkchoiceUpdatedRequest, ForkchoiceUpdatedResponse, GetPayloadRequest, PendingPayload};
use crate::storage::types::{Block, Transaction};
use crate::ev::h160_to_address;
use crate::{info, debug};
use alloy_primitives::{Address, U256, B256, hex};
use tonic::{Request, Response, Status};
use alloy_rlp::Decodable;
use evm::standard::TransactValueCallCreate;

impl MyTransactionService {
    pub async fn execute_transaction_impl(
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

        debug!("[RPC] DEBUG: Executing tx: from={:?}, to={:?}, value={}", from_addr, to_addr, val_u256);

        let block = Block::builder(next_number)
            .parent_hash(latest_block.body.execution_payload.block_hash)
            .timestamp(latest_block.body.execution_payload.timestamp + 12) // Simple block time increment
            .state_root(B256::ZERO)
            .add_transaction(tx.clone())
            .build();

        match self.executor.execute(&mut *storage, tx.clone(), block) {
            Ok(val) => {
                info!("[RPC] Transaction executed successfully: {:?}", tx.hash);
                let sender_balance = storage.get_balance(from_addr);
                debug!("[RPC] new balance for sender {}", sender_balance);

                if let Some(to_addr) = to_addr {
                    let receiver_balance = storage.get_balance(to_addr);
                    debug!("[RPC] new balance for receiver {}", receiver_balance);
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
                info!("[RPC] Transaction failed: {:?}, error: {}", tx.hash, e);
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



    pub async fn propose_block_impl(
        &self,
        request: Request<ProposeBlockRequest>,
    ) -> Result<Response<ProposeBlockResponse>, Status> {
        let req = request.into_inner();
        let mut storage = self.storage.lock().await;
        let latest_block = storage.get_latest_block().cloned().expect("Genesis block should exist");

        let parent_hash = if req.parent_hash.is_empty() {
            latest_block.body.execution_payload.block_hash
        } else {
            req.parent_hash.parse().map_err(|_| Status::invalid_argument("Invalid parent hash"))?
        };

        let timestamp = if req.timestamp == 0 {
            latest_block.body.execution_payload.timestamp + 12
        } else {
            req.timestamp
        };

        let fee_recipient = if req.fee_recipient.is_empty() {
            latest_block.body.execution_payload.fee_recipient
        } else {
            req.fee_recipient.parse().map_err(|_| Status::invalid_argument("Invalid fee recipient"))?
        };

        let mut transactions = Vec::new();
        if req.from_mempool {
            let n = if req.max_transactions == 0 {
                100 // Default limit
            } else {
                req.max_transactions as usize
            };
            transactions = storage.mempool.pop_transactions(n);
            debug!("[RPC] DEBUG: Pulled {} transactions from mempool", transactions.len());
        }

        for tx_req in req.transactions {
            let from_addr: Address = tx_req.from.parse().map_err(|_| Status::invalid_argument("Invalid from address"))?;
            let to_addr: Option<Address> = if tx_req.to.is_empty() {
                None
            } else {
                Some(tx_req.to.parse().map_err(|_| Status::invalid_argument("Invalid to address"))?)
            };

            let val_u256 = U256::from_str_radix(&tx_req.value, 10).or_else(|_| {
                U256::from_str_radix(tx_req.value.trim_start_matches("0x"), 16)
            }).map_err(|_| Status::invalid_argument("Invalid value"))?;

            let tx = Transaction::builder(from_addr)
                .nonce(tx_req.nonce)
                .to(to_addr)
                .value(val_u256)
                .data(tx_req.data)
                .gas_limit(tx_req.gas_limit)
                .gas_price(U256::from(tx_req.gas_price))
                .build();
            transactions.push(tx);
        }

        if transactions.is_empty() {
            return Err(Status::invalid_argument("No transactions provided and mempool is empty"));
        }

        let block = Block::builder(req.slot)
            .parent_hash(parent_hash)
            .timestamp(timestamp)
            .fee_recipient(fee_recipient)
            .state_root(B256::ZERO) // To be calculated
            .build();

        match self.executor.execute_block(&mut *storage, transactions, block) {
            Ok(vals) => {
                let mut tx_results = Vec::new();
                for (i, val) in vals.into_iter().enumerate() {
                    let (contract_address, return_data) = match val.call_create {
                        TransactValueCallCreate::Call { retval, .. } => (String::new(), retval),
                        TransactValueCallCreate::Create { address, .. } => (format!("{:?}", h160_to_address(address)), Vec::new()),
                    };
                    tx_results.push(TransactionResponse {
                        success: true,
                        message: format!("Transaction {} executed successfully", i),
                        tx_hash: String::new(), // We could add hash if needed
                        contract_address,
                        return_data,
                    });
                }

                let latest_block = storage.get_latest_block().cloned().expect("Finalized block should exist");

                // Broadcast the new block to the network
                if let Some(handle) = &self.network_handle {
                    let _ = handle.network_send.send(crate::network::NetworkMessage::BroadcastBlock(latest_block.clone())).await;
                }

                Ok(Response::new(ProposeBlockResponse {
                    success: true,
                    block_hash: format!("{:?}", latest_block.body.execution_payload.block_hash),
                    message: "Block proposed and executed successfully".to_string(),
                    tx_results,
                }))
            }
            Err(e) => {
                Ok(Response::new(ProposeBlockResponse {
                    success: false,
                    block_hash: String::new(),
                    message: format!("Block proposal failed: {}", e),
                    tx_results: Vec::new(),
                }))
            }
        }
    }

    pub async fn engine_new_payload_impl(
        &self,
        request: Request<ExecutionPayload>,
    ) -> Result<Response<PayloadStatus>, Status> {
        let payload = request.into_inner();
        let mut storage = self.storage.lock().await;

        let parent_hash: B256 = payload.parent_hash.parse().map_err(|_| Status::invalid_argument("Invalid parent hash"))?;
        let block_hash: B256 = payload.block_hash.parse().map_err(|_| Status::invalid_argument("Invalid block hash"))?;

        // Basic validation: check if parent exists
        if storage.get_block_by_hash(parent_hash).is_none() && parent_hash != B256::ZERO {
            return Ok(Response::new(PayloadStatus {
                status: "ACCEPTED".to_string(),
                latest_valid_hash: format!("{:?}", storage.head_block_hash),
                validation_error: "Parent block not found".to_string(),
            }));
        }

        let mut transactions = Vec::new();

        // Check if block hash matches any in pending_payloads
        {
            let mut pending = self.pending_payloads.lock().await;
            let mut found_id = None;
            for (id, payload) in pending.iter() {
                if payload.block.body.execution_payload.block_hash == block_hash {
                    found_id = Some(id.clone());
                    break;
                }
            }
            if let Some(id) = found_id {
                if let Some(payload) = pending.remove(&id) {
                    debug!("[RPC] DEBUG: Using pre-executed block from pending payload {}", id);
                    storage.backend.apply_overlayed(&payload.total_changeset);

                    // Decode transactions and add them to storage
                    for tx in &payload.block.body.execution_payload.transactions {
                        storage.add_transaction(tx.clone());
                    }

                    // Add receipts from block body
                    for (i, tx) in payload.block.body.execution_payload.transactions.iter().enumerate() {
                        if let Some(receipt) = payload.receipts.get(i) {
                            storage.add_receipt(tx.hash, receipt.clone());
                        }
                    }

                    storage.add_block(payload.block);
                    return Ok(Response::new(PayloadStatus {
                        status: "VALID".to_string(),
                        latest_valid_hash: format!("{:?}", block_hash),
                        validation_error: String::new(),
                    }));
                }
            }
        }
        for tx_bytes in &payload.transactions {
            let data = tx_bytes.clone();
            match Transaction::decode(&mut data.as_slice()) {
                Ok(tx) => transactions.push(tx),
                Err(e) => {
                    debug!("[RPC] DEBUG: Failed to decode transaction: {}", e);
                    return Ok(Response::new(PayloadStatus {
                        status: "INVALID".to_string(),
                        latest_valid_hash: format!("{:?}", storage.head_block_hash),
                        validation_error: format!("Failed to decode transaction: {}", e),
                    }));
                }
            }
        }

        // Construct block from payload
        let block = Block::builder(payload.block_number)
            .parent_hash(parent_hash)
            .timestamp(payload.timestamp)
            .fee_recipient(payload.fee_recipient.parse().unwrap_or_default())
            .state_root(payload.state_root.parse().unwrap_or_default())
            .transactions_root(payload.transactions_root.parse().unwrap_or_default())
            .receipts_root(payload.receipts_root.parse().unwrap_or_default())
            .withdrawals_root(payload.withdrawals_root.parse().unwrap_or_default())
            .prev_randao(payload.prev_randao.parse().unwrap_or_default())
            .gas_limit(payload.gas_limit)
            .gas_used(payload.gas_used)
            .block_hash(block_hash)
            .transactions(transactions.clone())
            .build();

        // Validate block by executing it
        let executor = crate::executor::Executor::new();
        match executor.execute_block(&mut storage, transactions, block.clone()) {
            Ok(_) => {
                storage.add_block(block.clone());

                // Broadcast the new block to the network
                if let Some(handle) = &self.network_handle {
                    let _ = handle.network_send.send(crate::network::NetworkMessage::BroadcastBlock(block)).await;
                }

                Ok(Response::new(PayloadStatus {
                    status: "VALID".to_string(),
                    latest_valid_hash: format!("{:?}", block_hash),
                    validation_error: String::new(),
                }))
            }
            Err(e) => {
                Ok(Response::new(PayloadStatus {
                    status: "INVALID".to_string(),
                    latest_valid_hash: format!("{:?}", storage.head_block_hash),
                    validation_error: format!("Block execution failed: {}", e),
                }))
            }
        }
    }

    pub async fn engine_forkchoice_updated_impl(
        &self,
        request: Request<ForkchoiceUpdatedRequest>,
    ) -> Result<Response<ForkchoiceUpdatedResponse>, Status> {
        let req = request.into_inner();
        let mut storage = self.storage.lock().await;

        let head_hash: B256 = req.forkchoice_state.as_ref().map(|s| s.head_block_hash.parse().unwrap_or_default()).unwrap_or_default();
        let safe_hash: B256 = req.forkchoice_state.as_ref().map(|s| s.safe_block_hash.parse().unwrap_or_default()).unwrap_or_default();
        let finalized_hash: B256 = req.forkchoice_state.as_ref().map(|s| s.finalized_block_hash.parse().unwrap_or_default()).unwrap_or_default();

        storage.head_block_hash = head_hash;
        storage.safe_block_hash = safe_hash;
        storage.finalized_block_hash = finalized_hash;

        debug!("[RPC] DEBUG: Forkchoice updated: head={:?}, safe={:?}, finalized={:?}", head_hash, safe_hash, finalized_hash);

        // Transactional Rollback: if the new head hash is different, we might want to return
        // transactions from pending payloads that were building on the old head.
        // For simplicity, if head changes, we clear pending payloads and return their transactions to mempool.
        {
            let mut pending = self.pending_payloads.lock().await;
            let mut to_rollback = Vec::new();
            for (id, payload) in pending.iter() {
                if payload.block.body.execution_payload.parent_hash != head_hash {
                    to_rollback.push(id.clone());
                }
            }
            for id in to_rollback {
                if let Some(payload) = pending.remove(&id) {
                    debug!("[RPC] DEBUG: Rolling back transactions from pending payload {}", id);
                    for tx in &payload.block.body.execution_payload.transactions {
                        storage.mempool.add_transaction(tx.clone());
                    }
                }
            }
        }

        let mut payload_id = String::new();
        if let Some(attr) = req.payload_attributes {
            payload_id = format!("0x{:x}", rand::random::<u64>());
            debug!("[RPC] DEBUG: Starting block building, payload_id={}", payload_id);

            let latest_block = storage.get_block_by_hash(head_hash).cloned().unwrap_or_else(|| storage.get_latest_block().cloned().unwrap());

            let next_number = latest_block.body.execution_payload.block_number + 1;
            let txs = storage.mempool.pop_transactions(10);

            let block_to_execute = Block::builder(next_number)
                .parent_hash(head_hash)
                .timestamp(attr.timestamp)
                .fee_recipient(attr.suggested_fee_recipient.parse().unwrap_or_default())
                .prev_randao(attr.prev_randao.parse().unwrap_or_default())
                .transactions(txs.clone())
                .build();

            // Execute the block to get the state changes
            match self.executor.execute_with_changeset(&mut storage, txs, block_to_execute.clone()) {
                Ok((_results, receipts, total_changeset)) => {
                    // Finalize the block with calculated roots
                    let cumulative_gas_used = receipts.last().map(|r| r.cumulative_gas_used).unwrap_or(0);
                    let mut finalized_block_builder = Block::builder(block_to_execute.slot)
                        .parent_hash(block_to_execute.body.execution_payload.parent_hash)
                        .timestamp(block_to_execute.body.execution_payload.timestamp)
                        .fee_recipient(block_to_execute.body.execution_payload.fee_recipient)
                        .prev_randao(block_to_execute.body.execution_payload.prev_randao)
                        .block_number(block_to_execute.body.execution_payload.block_number)
                        .gas_used(cumulative_gas_used)
                        .transactions(block_to_execute.body.execution_payload.transactions.clone());

                    for receipt in &receipts {
                        finalized_block_builder = finalized_block_builder.add_receipt(receipt.clone());
                    }

                    self.pending_payloads.lock().await.insert(payload_id.clone(), PendingPayload {
                        block: finalized_block_builder.build(),
                        receipts,
                        total_changeset,
                    });
                }
                Err(e) => {
                    debug!("[RPC] DEBUG: Failed to build block: {}", e);
                    // If building fails, payload_id remains empty or we handle it differently
                    payload_id = String::new();
                }
            }
        }

        Ok(Response::new(ForkchoiceUpdatedResponse {
            payload_status: Some(PayloadStatus {
                status: "VALID".to_string(),
                latest_valid_hash: format!("{:?}", head_hash),
                validation_error: String::new(),
            }),
            payload_id,
        }))
    }

    pub async fn engine_get_payload_impl(
        &self,
        request: Request<GetPayloadRequest>,
    ) -> Result<Response<ExecutionPayload>, Status> {
        let req = request.into_inner();
        let pending = self.pending_payloads.lock().await;

        let pending_payload = pending.get(&req.payload_id).ok_or_else(|| Status::not_found("Payload not found"))?;
        let block = &pending_payload.block;
        let payload = block.body.execution_payload.clone();

        Ok(Response::new(ExecutionPayload {
            parent_hash: format!("{:?}", payload.parent_hash),
            fee_recipient: format!("{:?}", payload.fee_recipient),
            state_root: format!("{:?}", payload.state_root),
            receipts_root: format!("{:?}", payload.receipts_root),
            logs_bloom: hex::encode(payload.logs_bloom),
            prev_randao: format!("{:?}", payload.prev_randao),
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data,
            base_fee_per_gas: payload.base_fee_per_gas.to_string(),
            block_hash: format!("{:?}", payload.block_hash),
            transactions: payload.transactions.iter().map(|_| Vec::new()).collect(),
            withdrawals: Vec::new(),
            blob_gas_used: 0,
            excess_blob_gas: 0,
            transactions_root: format!("{:?}",payload.transactions_root),
            withdrawals_root: format!("{:?}",payload.withdrawals_root)
        }))
    }
}


