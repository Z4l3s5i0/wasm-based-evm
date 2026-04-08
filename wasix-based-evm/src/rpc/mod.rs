pub mod net;
pub mod eth;
pub mod engine;

pub mod evm_rpc {
    tonic::include_proto!("evm_rpc");
}

pub use evm_rpc::transaction_service_server::TransactionService;
pub use evm_rpc::*;
use crate::executor::Executor;
use crate::network::NetworkHandle;
use std::sync::Arc;
use alloy_rlp::Decodable;
use tokio::sync::{Mutex};
use tonic::{Request, Response, Status};


use crate::storage::storage::InMemoryStorage;
use crate::storage::types::{Block, Receipt};

pub struct PendingPayload {
    pub block: Block,
    pub receipts: Vec<Receipt>,
    pub total_changeset: evm::backend::OverlayedChangeSet,
}

pub struct MyTransactionService {
    pub storage: Arc<Mutex<InMemoryStorage>>,
    pub executor: Executor,
    pub pending_payloads: Arc<Mutex<std::collections::HashMap<String, PendingPayload>>>,
    pub network_handle: Option<NetworkHandle>,
}
#[tonic::async_trait]
impl TransactionService for MyTransactionService {
    async fn execute_transaction(&self, r: Request<TransactionRequest>) -> Result<Response<TransactionResponse>, Status> {
        self.execute_transaction_impl(r).await
    }
    async fn eth_accounts(&self, r: Request<Empty>) -> Result<Response<AccountsResponse>, Status> {
        self.eth_accounts_impl(r).await
    }
    async fn eth_block_number(&self, r: Request<Empty>) -> Result<Response<BlockNumberResponse>, Status> {
        self.eth_block_number_impl(r).await
    }
    async fn eth_gas_price(&self, r: Request<Empty>) -> Result<Response<GasPriceResponse>, Status> {
        self.eth_gas_price_impl(r).await
    }
    async fn eth_get_balance(&self, r: Request<GetBalanceRequest>) -> Result<Response<BalanceResponse>, Status> {
        self.eth_get_balance_impl(r).await
    }
    async fn eth_get_block_by_number(&self, r: Request<GetBlockByNumberRequest>) -> Result<Response<BlockResponse>, Status> {
        self.eth_get_block_by_number_impl(r).await
    }
    async fn eth_get_block_by_hash(&self, r: Request<GetBlockByHashRequest>) -> Result<Response<BlockResponse>, Status> {
        self.eth_get_block_by_hash_impl(r).await
    }
    async fn eth_get_block_transaction_count_by_hash(&self, r: Request<GetBlockTransactionCountByHashRequest>) -> Result<Response<TransactionCountResponse>, Status> {
        self.eth_get_block_transaction_count_by_hash_impl(r).await
    }
    async fn eth_get_block_transaction_count_by_number(&self, r: Request<GetBlockTransactionCountByNumberRequest>) -> Result<Response<TransactionCountResponse>, Status> {
        self.eth_get_block_transaction_count_by_number_impl(r).await
    }
    async fn eth_get_transaction_by_hash(&self, r: Request<GetTransactionByHashRequest>) -> Result<Response<TransactionInfoResponse>, Status> {
        self.eth_get_transaction_by_hash_impl(r).await
    }
    async fn eth_get_transaction_receipt(&self, r: Request<GetTransactionByHashRequest>) -> Result<Response<TransactionReceiptResponse>, Status> {
        self.eth_get_transaction_receipt_impl(r).await
    }
    async fn eth_send_transaction(&self, r: Request<TransactionRequest>) -> Result<Response<TransactionResponse>, Status> {
        self.eth_send_transaction_impl(r).await
    }
    async fn eth_call(&self, r: Request<TransactionRequest>) -> Result<Response<TransactionResponse>, Status> {
        self.eth_call_impl(r).await
    }
    async fn eth_get_code(&self, r: Request<GetCodeRequest>) -> Result<Response<CodeResponse>, Status> {
        self.eth_get_code_impl(r).await
    }
    async fn eth_get_roots(&self, r: Request<Empty>) -> Result<Response<RootsResponse>, Status> {
        self.eth_get_roots_impl(r).await
    }
    async fn eth_get_mempool(&self, r: Request<Empty>) -> Result<Response<MempoolResponse>, Status> {
        self.eth_get_mempool_impl(r).await
    }
    async fn propose_block(&self, r: Request<ProposeBlockRequest>) -> Result<Response<ProposeBlockResponse>, Status> {
        self.propose_block_impl(r).await
    }
    async fn engine_new_payload(&self, r: Request<ExecutionPayload>) -> Result<Response<PayloadStatus>, Status> {
        self.engine_new_payload_impl(r).await
    }
    async fn engine_forkchoice_updated(&self, r: Request<ForkchoiceUpdatedRequest>) -> Result<Response<ForkchoiceUpdatedResponse>, Status> {
        self.engine_forkchoice_updated_impl(r).await
    }
    async fn engine_get_payload(&self, r: Request<GetPayloadRequest>) -> Result<Response<ExecutionPayload>, Status> {
        self.engine_get_payload_impl(r).await
    }
    async fn net_peer_count(&self, r: Request<Empty>) -> Result<Response<NetPeerCountResponse>, Status> {
        self.net_peer_count_impl(r).await
    }
    async fn net_peers(&self, r: Request<Empty>) -> Result<Response<NetPeersResponse>, Status> {
        self.net_peers_impl(r).await
    }
    async fn net_add_peer(&self, r: Request<NetAddPeerRequest>) -> Result<Response<NetAddPeerResponse>, Status> {
        self.net_add_peer_impl(r).await
    }
    async fn net_node_info(&self, r: Request<Empty>) -> Result<Response<NetNodeInfoResponse>, Status> {
        self.net_node_info_impl(r).await
    }
}

