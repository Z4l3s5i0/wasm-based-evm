use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::{Mutex, oneshot, mpsc};
use tonic::{Response, Status};
use alloy_primitives::{Address, B256};

use crate::executor::Executor;
use crate::storage::storage::InMemoryStorage;
use crate::storage::types::{Block, Transaction};
use crate::storage::{Receipt, Withdrawal};
use crate::rpc::{
    AccountsResponse, BalanceResponse, BlockResponse, CodeResponse, Empty, ExecutionPayload,
    ForkchoiceUpdatedRequest, ForkchoiceUpdatedResponse, GasPriceResponse, GetBalanceRequest,
    GetBlockByHashRequest, GetBlockByNumberRequest, GetCodeRequest, GetPayloadRequest,
    GetTransactionByHashRequest, LogEntry, MempoolResponse, NetAddPeerRequest, NetAddPeerResponse,
    NetNodeInfoResponse, NetPeerCountResponse, NetPeersResponse, PayloadStatus, ProposeBlockRequest,
    ProposeBlockResponse, RootsResponse, TransactionCountResponse, TransactionInfoResponse,
    TransactionReceiptResponse, TransactionRequest, TransactionResponse,
};
use crate::rpc::PendingPayload;
use crate::rpc::PeerInfo as ProtoPeerInfo;
use crate::network::{NetworkMessage};
use evm::standard::TransactValueCallCreate;
use crate::ev::h160_to_address;
use alloy_primitives::{U256, hex};
use alloy_rlp::Decodable;

#[async_trait]
pub trait BlockchainProvider {
    // Eth namespace (read)
    async fn accounts(&self) -> Result<Response<AccountsResponse>, Status>;
    async fn latest_block_number(&self) -> Result<Response<BlockNumberResponse>, Status>;
    async fn balance(&self, address: Address) -> Result<Response<BalanceResponse>, Status>;
    async fn block_by_number(&self, number: u64) -> Result<Response<BlockResponse>, Status>;
    async fn block_by_hash(&self, hash: B256) -> Result<Response<BlockResponse>, Status>;
    async fn block_transaction_count_by_hash(&self, hash: B256) -> Result<Response<TransactionCountResponse>, Status>;
    async fn block_transaction_count_by_number(&self, number: u64) -> Result<Response<TransactionCountResponse>, Status>;
    async fn tx_by_hash(&self, hash: B256) -> Result<Response<TransactionInfoResponse>, Status>;
    async fn tx_receipt_by_hash(&self, hash: B256) -> Result<Response<TransactionReceiptResponse>, Status>;
    async fn code_at(&self, address: Address) -> Result<Response<CodeResponse>, Status>;
    async fn roots(&self) -> Result<Response<RootsResponse>, Status>;
    async fn mempool(&self) -> Result<Response<MempoolResponse>, Status>;

    // Eth namespace (write/exec)
    async fn send_transaction(&self, req: TransactionRequest) -> Result<Response<TransactionResponse>, Status>;
    async fn call(&self, req: TransactionRequest) -> Result<Response<TransactionResponse>, Status>;

    // Engine API
    async fn propose_block(&self, req: ProposeBlockRequest) -> Result<Response<ProposeBlockResponse>, Status>;
    async fn engine_new_payload(&self, payload: ExecutionPayload) -> Result<Response<PayloadStatus>, Status>;
    async fn engine_forkchoice_updated(&self, req: ForkchoiceUpdatedRequest) -> Result<Response<ForkchoiceUpdatedResponse>, Status>;
    async fn engine_get_payload(&self, req: GetPayloadRequest) -> Result<Response<ExecutionPayload>, Status>;

    // Net namespace
    async fn net_peer_count(&self) -> Result<Response<NetPeerCountResponse>, Status>;
    async fn net_peers(&self) -> Result<Response<NetPeersResponse>, Status>;
    async fn net_add_peer(&self, _req: NetAddPeerRequest) -> Result<Response<NetAddPeerResponse>, Status>;
    async fn net_node_info(&self) -> Result<Response<NetNodeInfoResponse>, Status>;
}

// Note: This skeleton implementation will progressively move logic out of MyTransactionService.
pub struct DefaultBlockchainProvider {
    storage: Arc<Mutex<InMemoryStorage>>,
    executor: Executor,
    // Optional network sender for net_* namespace delegation
    network_send: Option<mpsc::Sender<NetworkMessage>>,
    // Optional direct tx broadcast channel (legacy path used by NetworkHandle)
    tx_broadcast: Option<mpsc::Sender<Transaction>>,
    // Shared pending payloads map (engine API building pipeline)
    pending_payloads: Arc<Mutex<std::collections::HashMap<String, PendingPayload>>>,
}

impl DefaultBlockchainProvider {
    pub fn new(
        storage: Arc<Mutex<InMemoryStorage>>,
        executor: Executor,
        network_send: Option<mpsc::Sender<NetworkMessage>>,
        tx_broadcast: Option<mpsc::Sender<Transaction>>,
        pending_payloads: Arc<Mutex<std::collections::HashMap<String, PendingPayload>>>,
    ) -> Self {
        Self { storage, executor, network_send, tx_broadcast, pending_payloads }
    }
}

use crate::rpc::{BlockNumberResponse};

#[async_trait]
impl BlockchainProvider for DefaultBlockchainProvider {
    async fn accounts(&self) -> Result<Response<AccountsResponse>, Status> {
        let storage = self.storage.lock().await;
        let accounts = storage.get_accounts();
        Ok(Response::new(AccountsResponse { addresses: accounts.into_iter().map(|a| format!("{:?}", a)).collect() }))
    }

    async fn latest_block_number(&self) -> Result<Response<BlockNumberResponse>, Status> {
        let storage = self.storage.lock().await;
        Ok(Response::new(BlockNumberResponse { number: storage.get_latest_block_number() }))
    }

    async fn balance(&self, address: Address) -> Result<Response<BalanceResponse>, Status> {
        let storage = self.storage.lock().await;
        Ok(Response::new(BalanceResponse { balance: storage.get_balance(address).to_string() }))
    }

    async fn block_by_number(&self, number: u64) -> Result<Response<BlockResponse>, Status> {
        let storage = self.storage.lock().await;
        let block = storage.get_block_by_number(number).ok_or_else(|| Status::not_found("Block not found"))?;
        Ok(Response::new(BlockResponse::from(block)))
    }

    async fn block_by_hash(&self, hash: B256) -> Result<Response<BlockResponse>, Status> {
        let storage = self.storage.lock().await;
        let block = storage.get_block_by_hash(hash).ok_or_else(|| Status::not_found("Block not found"))?;
        Ok(Response::new(BlockResponse::from(block)))
    }

    async fn block_transaction_count_by_hash(&self, hash: B256) -> Result<Response<TransactionCountResponse>, Status> {
        let storage = self.storage.lock().await;
        let block = storage
            .get_block_by_hash(hash)
            .ok_or_else(|| Status::not_found("Block not found"))?;
        Ok(Response::new(TransactionCountResponse {
            count: block.body.execution_payload.transactions.len() as u64,
        }))
    }

    async fn block_transaction_count_by_number(&self, number: u64) -> Result<Response<TransactionCountResponse>, Status> {
        let storage = self.storage.lock().await;
        let block = storage
            .get_block_by_number(number)
            .ok_or_else(|| Status::not_found("Block not found"))?;
        Ok(Response::new(TransactionCountResponse {
            count: block.body.execution_payload.transactions.len() as u64,
        }))
    }

    async fn tx_by_hash(&self, hash: B256) -> Result<Response<TransactionInfoResponse>, Status> {
        let storage = self.storage.lock().await;
        let tx = storage
            .get_transaction_by_hash(hash)
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
            gas_price: tx.gas_price.as_limbs()[0],
            block_number: block.map(|b| b.body.execution_payload.block_number).unwrap_or(0),
            block_hash: block
                .map(|b| format!("{:?}", b.body.execution_payload.block_hash))
                .unwrap_or_default(),
        }))
    }

    async fn tx_receipt_by_hash(
        &self,
        hash: B256,
    ) -> Result<Response<TransactionReceiptResponse>, Status> {
        let storage = self.storage.lock().await;

        let tx = storage
            .get_transaction_by_hash(hash)
            .ok_or_else(|| Status::not_found("Transaction not found"))?;
        let receipt = storage
            .get_receipt_by_tx_hash(hash)
            .ok_or_else(|| Status::not_found("Receipt not found"))?;
        let block = storage
            .get_block_by_transaction_hash(hash)
            .ok_or_else(|| Status::not_found("Block for transaction not found"))?;

        // Build logs
        let logs: Vec<LogEntry> = receipt
            .logs
            .iter()
            .enumerate()
            .map(|(i, l)| LogEntry {
                address: format!("{:?}", l.address),
                topics: l.topics.iter().map(|t| format!("{:?}", t)).collect(),
                data: l.data.clone(),
                block_number: block.body.execution_payload.block_number,
                block_hash: format!("{:?}", block.body.execution_payload.block_hash),
                transaction_hash: format!("{:?}", tx.hash),
                transaction_index: block
                    .body
                    .execution_payload
                    .transactions
                    .iter()
                    .position(|t| t.hash == tx.hash)
                    .unwrap_or(0) as u64,
                log_index: i as u64,
            })
            .collect();

        let resp = TransactionReceiptResponse {
            transaction_hash: format!("{:?}", tx.hash),
            transaction_index: block
                .body
                .execution_payload
                .transactions
                .iter()
                .position(|t| t.hash == tx.hash)
                .unwrap_or(0) as u64,
            block_hash: format!("{:?}", block.body.execution_payload.block_hash),
            block_number: block.body.execution_payload.block_number,
            from: format!("{:?}", tx.from),
            to: tx.to.map(|a| format!("{:?}", a)).unwrap_or_default(),
            cumulative_gas_used: receipt.cumulative_gas_used,
            gas_used: receipt.cumulative_gas_used, // Simplified: no per-tx gas used tracked yet
            contract_address: String::new(),
            logs,
            logs_bloom: format!("{:?}", receipt.logs_bloom),
            status: if receipt.success { 1 } else { 0 },
        };

        Ok(Response::new(resp))
    }

    async fn code_at(&self, address: Address) -> Result<Response<CodeResponse>, Status> {
        let storage = self.storage.lock().await;
        let code = storage.get_code(address);
        Ok(Response::new(CodeResponse { code }))
    }

    async fn roots(&self) -> Result<Response<RootsResponse>, Status> {
        let storage = self.storage.lock().await;
        let block = storage
            .get_latest_block()
            .cloned()
            .expect("Genesis block should exist");
        let hash = block.body.execution_payload.block_hash;

        let calculated_state_root = storage.calculate_state_root();
        let calculated_tx_root = Transaction::calculate_root(&block.body.execution_payload.transactions);
        let calculated_receipts_root = Receipt::calculate_root(&storage.get_block_receipts(hash));
        let calculated_withdrawals_root = Withdrawal::calculate_root(&block.body.execution_payload.withdrawals);

        Ok(Response::new(RootsResponse {
            state_root: format!(
                "{:?} (calc: {:?})",
                block.body.execution_payload.state_root, calculated_state_root
            ),
            transactions_root: format!(
                "{:?} (calc: {:?})",
                block.body.execution_payload.transactions_root, calculated_tx_root
            ),
            receipts_root: format!(
                "{:?} (calc: {:?})",
                block.body.execution_payload.receipts_root, calculated_receipts_root
            ),
            withdrawals_root: format!(
                "{:?} (calc: {:?})",
                block.body.execution_payload.withdrawals_root, calculated_withdrawals_root
            ),
        }))
    }

    async fn mempool(&self) -> Result<Response<MempoolResponse>, Status> {
        let storage = self.storage.lock().await;
        let mempool_txs = storage.mempool.get_all_transactions();

        let transactions = mempool_txs
            .into_iter()
            .map(|tx| TransactionInfoResponse {
                hash: format!("{:?}", tx.hash),
                nonce: tx.nonce,
                from: format!("{:?}", tx.from),
                to: tx.to.map(|a| format!("{:?}", a)).unwrap_or_default(),
                value: tx.value.to_string(),
                data: tx.data.clone(),
                gas_limit: tx.gas_limit,
                gas_price: tx.gas_price.as_limbs()[0],
                block_number: 0,
                block_hash: String::new(),
            })
            .collect();

        Ok(Response::new(MempoolResponse { transactions }))
    }

    async fn send_transaction(&self, req: TransactionRequest) -> Result<Response<TransactionResponse>, Status> {
        // Map RPC request to internal transaction
        let tx = Transaction::try_from(req)?;

        let mut storage = self.storage.lock().await;
        let tx_hash = tx.hash;
        storage.mempool.add_transaction(tx.clone());

        // Broadcast to P2P network if available
        if let Some(tx_chan) = &self.tx_broadcast {
            let _ = tx_chan.send(tx).await; // best-effort
        }

        Ok(Response::new(TransactionResponse {
            success: true,
            message: "Transaction added to mempool and broadcasted".to_string(),
            tx_hash: format!("{:?}", tx_hash),
            contract_address: String::new(),
            return_data: Vec::new(),
        }))
    }

    async fn call(&self, req: TransactionRequest) -> Result<Response<TransactionResponse>, Status> {
        let tx = Transaction::try_from(req)?;

        let storage = self.storage.lock().await;
        let latest_block = storage.get_latest_block().cloned().expect("Genesis block should exist");

        match self.executor.call(&*storage, tx.clone(), latest_block) {
            Ok(val) => {
                let (_contract_addr, return_data) = match val.call_create {
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

    async fn propose_block(&self, req: ProposeBlockRequest) -> Result<Response<ProposeBlockResponse>, Status> {
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
            let n = if req.max_transactions == 0 { 100 } else { req.max_transactions as usize };
            transactions = storage.mempool.pop_transactions(n);
        }

        for tx_req in req.transactions {
            let from_addr: Address = tx_req.from.parse().map_err(|_| Status::invalid_argument("Invalid from address"))?;
            let to_addr: Option<Address> = if tx_req.to.is_empty() { None } else { Some(tx_req.to.parse().map_err(|_| Status::invalid_argument("Invalid to address"))?) };
            let val_u256 = U256::from_str_radix(&tx_req.value, 10).or_else(|_| U256::from_str_radix(tx_req.value.trim_start_matches("0x"), 16)).map_err(|_| Status::invalid_argument("Invalid value"))?;
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
            .state_root(B256::ZERO)
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
                        tx_hash: String::new(),
                        contract_address,
                        return_data,
                    });
                }

                let latest_block = storage.get_latest_block().cloned().expect("Finalized block should exist");

                if let Some(handle) = &self.network_send {
                    let _ = handle.send(NetworkMessage::BroadcastBlock(latest_block.clone())).await;
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

    async fn engine_new_payload(&self, payload: ExecutionPayload) -> Result<Response<PayloadStatus>, Status> {
        let mut storage = self.storage.lock().await;

        let parent_hash: B256 = payload.parent_hash.parse().map_err(|_| Status::invalid_argument("Invalid parent hash"))?;
        let block_hash: B256 = payload.block_hash.parse().map_err(|_| Status::invalid_argument("Invalid block hash"))?;

        if storage.get_block_by_hash(parent_hash).is_none() && parent_hash != B256::ZERO {
            return Ok(Response::new(PayloadStatus {
                status: "ACCEPTED".to_string(),
                latest_valid_hash: format!("{:?}", storage.head_block_hash),
                validation_error: "Parent block not found".to_string(),
            }));
        }

        let mut transactions = Vec::new();

        // Check if block hash matches any pre-built pending payload
        {
            let mut pending = self.pending_payloads.lock().await;
            let mut found_id = None;
            for (id, p) in pending.iter() {
                if p.block.body.execution_payload.block_hash == block_hash {
                    found_id = Some(id.clone());
                    break;
                }
            }
            if let Some(id) = found_id {
                if let Some(p) = pending.remove(&id) {
                    storage.backend.apply_overlayed(&p.total_changeset);
                    for tx in &p.block.body.execution_payload.transactions { storage.add_transaction(tx.clone()); }
                    for (i, tx) in p.block.body.execution_payload.transactions.iter().enumerate() {
                        if let Some(rcpt) = p.receipts.get(i) { storage.add_receipt(tx.hash, rcpt.clone()); }
                    }
                    storage.add_block(p.block);
                    return Ok(Response::new(PayloadStatus { status: "VALID".to_string(), latest_valid_hash: format!("{:?}", block_hash), validation_error: String::new() }));
                }
            }
        }

        for tx_bytes in &payload.transactions {
            let data = tx_bytes.clone();
            match Transaction::decode(&mut data.as_slice()) {
                Ok(tx) => transactions.push(tx),
                Err(e) => {
                    return Ok(Response::new(PayloadStatus {
                        status: "INVALID".to_string(),
                        latest_valid_hash: format!("{:?}", storage.head_block_hash),
                        validation_error: format!("Failed to decode transaction: {}", e),
                    }));
                }
            }
        }

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

        match self.executor.execute_block(&mut *storage, transactions, block.clone()) {
            Ok(_) => {
                storage.add_block(block.clone());
                if let Some(handle) = &self.network_send { let _ = handle.send(NetworkMessage::BroadcastBlock(block)).await; }
                Ok(Response::new(PayloadStatus { status: "VALID".to_string(), latest_valid_hash: format!("{:?}", block_hash), validation_error: String::new() }))
            }
            Err(e) => {
                Ok(Response::new(PayloadStatus { status: "INVALID".to_string(), latest_valid_hash: format!("{:?}", storage.head_block_hash), validation_error: format!("Block execution failed: {}", e) }))
            }
        }
    }

    async fn engine_forkchoice_updated(&self, req: ForkchoiceUpdatedRequest) -> Result<Response<ForkchoiceUpdatedResponse>, Status> {
        let mut storage = self.storage.lock().await;

        let head_hash: B256 = req.forkchoice_state.as_ref().map(|s| s.head_block_hash.parse().unwrap_or_default()).unwrap_or_default();
        let safe_hash: B256 = req.forkchoice_state.as_ref().map(|s| s.safe_block_hash.parse().unwrap_or_default()).unwrap_or_default();
        let finalized_hash: B256 = req.forkchoice_state.as_ref().map(|s| s.finalized_block_hash.parse().unwrap_or_default()).unwrap_or_default();

        storage.head_block_hash = head_hash;
        storage.safe_block_hash = safe_hash;
        storage.finalized_block_hash = finalized_hash;

        // Rollback pending payloads not building on new head: return txs to mempool
        {
            let mut pending = self.pending_payloads.lock().await;
            let mut to_rollback = Vec::new();
            for (id, p) in pending.iter() {
                if p.block.body.execution_payload.parent_hash != head_hash { to_rollback.push(id.clone()); }
            }
            for id in to_rollback {
                if let Some(p) = pending.remove(&id) {
                    for tx in &p.block.body.execution_payload.transactions { storage.mempool.add_transaction(tx.clone()); }
                }
            }
        }

        let mut payload_id = String::new();
        if let Some(attr) = req.payload_attributes {
            payload_id = format!("0x{:x}", rand::random::<u64>());
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

            match self.executor.execute_with_changeset(&mut *storage, txs, block_to_execute.clone()) {
                Ok((_results, receipts, total_changeset)) => {
                    let cumulative_gas_used = receipts.last().map(|r| r.cumulative_gas_used).unwrap_or(0);
                    let mut finalized_block_builder = Block::builder(block_to_execute.slot)
                        .parent_hash(block_to_execute.body.execution_payload.parent_hash)
                        .timestamp(block_to_execute.body.execution_payload.timestamp)
                        .fee_recipient(block_to_execute.body.execution_payload.fee_recipient)
                        .prev_randao(block_to_execute.body.execution_payload.prev_randao)
                        .block_number(block_to_execute.body.execution_payload.block_number)
                        .gas_used(cumulative_gas_used)
                        .transactions(block_to_execute.body.execution_payload.transactions.clone());
                    for receipt in &receipts { finalized_block_builder = finalized_block_builder.add_receipt(receipt.clone()); }
                    self.pending_payloads.lock().await.insert(payload_id.clone(), PendingPayload { block: finalized_block_builder.build(), receipts, total_changeset });
                }
                Err(_e) => { payload_id = String::new(); }
            }
        }

        Ok(Response::new(ForkchoiceUpdatedResponse { payload_status: Some(PayloadStatus { status: "VALID".to_string(), latest_valid_hash: format!("{:?}", head_hash), validation_error: String::new() }), payload_id }))
    }

    async fn engine_get_payload(&self, req: GetPayloadRequest) -> Result<Response<ExecutionPayload>, Status> {
        let pending = self.pending_payloads.lock().await;
        let pending_payload = pending.get(&req.payload_id).ok_or_else(|| Status::not_found("Payload not found"))?;
        let payload = pending_payload.block.body.execution_payload.clone();
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
            transactions_root: format!("{:?}", payload.transactions_root),
            withdrawals_root: format!("{:?}", payload.withdrawals_root),
        }))
    }

    async fn net_peer_count(&self) -> Result<Response<NetPeerCountResponse>, Status> {
        let Some(handle) = &self.network_send else {
            return Err(Status::unavailable("Network not started"));
        };
        let (tx, rx) = oneshot::channel();
        handle.send(NetworkMessage::GetPeerCount(tx)).await.map_err(|_| Status::internal("Failed to send to network service"))?;
        let count = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
            .await.map_err(|_| Status::deadline_exceeded("Network service timed out"))?
            .map_err(|_| Status::internal("Failed to receive from network service"))?;
        Ok(Response::new(NetPeerCountResponse { count: count as u64 }))
    }

    async fn net_peers(&self) -> Result<Response<NetPeersResponse>, Status> {
        let Some(handle) = &self.network_send else {
            return Err(Status::unavailable("Network not started"));
        };
        let (tx, rx) = oneshot::channel();
        handle.send(NetworkMessage::GetPeers(tx)).await.map_err(|_| Status::internal("Failed to send to network service"))?;
        let peers = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
            .await.map_err(|_| Status::deadline_exceeded("Network service timed out"))?
            .map_err(|_| Status::internal("Failed to receive from network service"))?;
        let proto_peers = peers.into_iter().map(|p| ProtoPeerInfo { id: p.id, addr: p.addr, enr: p.enr.unwrap_or_default() }).collect();
        Ok(Response::new(NetPeersResponse { peers: proto_peers }))
    }

    async fn net_add_peer(&self, req: NetAddPeerRequest) -> Result<Response<NetAddPeerResponse>, Status> {
        let Some(handle) = &self.network_send else {
            return Err(Status::unavailable("Network not started"));
        };
        let (tx, rx) = oneshot::channel();
        handle.send(NetworkMessage::AddPeer(req.addr, tx)).await.map_err(|_| Status::internal("Failed to send to network service"))?;
        match rx.await.map_err(|_| Status::internal("Failed to receive from network service"))? {
            Ok(_) => Ok(Response::new(NetAddPeerResponse { success: true, message: "Peer added".to_string() })),
            Err(e) => Ok(Response::new(NetAddPeerResponse { success: false, message: e })),
        }
    }

    async fn net_node_info(&self) -> Result<Response<NetNodeInfoResponse>, Status> {
        let Some(handle) = &self.network_send else {
            return Err(Status::unavailable("Network not started"));
        };
        let (tx, rx) = oneshot::channel();
        handle.send(NetworkMessage::GetNodeInfo(tx)).await.map_err(|_| Status::internal("Failed to send to network service"))?;
        let info = rx.await.map_err(|_| Status::internal("Failed to receive from network service"))?;
        Ok(Response::new(NetNodeInfoResponse { enr: info.enr, node_id: info.node_id, listen_addresses: info.listen_addresses }))
    }
}
