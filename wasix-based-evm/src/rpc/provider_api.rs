use alloy_primitives::{Address, B256};
use crate::storage::types::{Block, Receipt, Transaction, ExecutionPayload};
use crate::rpc::provider_error::ProviderError;
use crate::rpc::evm_rpc::{TransactionRequest, ProposeBlockRequest, ForkchoiceUpdatedRequest, GetPayloadRequest};

#[async_trait::async_trait]
pub trait EthReadProvider: Send + Sync {
    async fn accounts(&self) -> Result<Vec<Address>, ProviderError>;
    async fn latest_block_number(&self) -> Result<u64, ProviderError>;
    async fn balance(&self, address: Address) -> Result<u128, ProviderError>;
    async fn block_by_number(&self, number: u64) -> Result<Option<Block>, ProviderError>;
    async fn block_by_hash(&self, hash: B256) -> Result<Option<Block>, ProviderError>;
    async fn block_transaction_count_by_number(&self, number: u64) -> Result<u64, ProviderError>;
    async fn block_transaction_count_by_hash(&self, hash: B256) -> Result<u64, ProviderError>;
    async fn tx_by_hash(&self, hash: B256) -> Result<Option<Transaction>, ProviderError>;
    async fn tx_receipt_by_hash(&self, hash: B256) -> Result<Option<(Transaction, Receipt, Block)>, ProviderError>;
    async fn code_at(&self, address: Address) -> Result<Vec<u8>, ProviderError>;
    async fn roots(&self) -> Result<(B256, B256, Option<B256>), ProviderError>;
    async fn mempool(&self) -> Result<Vec<Transaction>, ProviderError>;
}

#[async_trait::async_trait]
pub trait EthWriteProvider: Send + Sync {
    async fn send_transaction(&self, req: TransactionRequest) -> Result<Transaction, ProviderError>;
    async fn call(&self, req: TransactionRequest) -> Result<Transaction, ProviderError>;
}

#[async_trait::async_trait]
pub trait EngineProvider: Send + Sync {
    async fn propose_block(&self, req: ProposeBlockRequest) -> Result<ProposeBlockResult, ProviderError>;
    async fn engine_new_payload(&self, payload: crate::rpc::evm_rpc::ExecutionPayload) -> Result<crate::rpc::evm_rpc::PayloadStatus, ProviderError>;
    async fn engine_forkchoice_updated(&self, req: ForkchoiceUpdatedRequest) -> Result<crate::rpc::evm_rpc::ForkchoiceUpdatedResponse, ProviderError>;
    async fn engine_get_payload(&self, req: GetPayloadRequest) -> Result<crate::rpc::evm_rpc::ExecutionPayload, ProviderError>;
}

pub struct ProposeBlockResult {
    pub block_hash: B256,
    pub tx_results: Vec<Transaction>,
}

#[async_trait::async_trait]
pub trait NetProvider: Send + Sync {
    async fn peer_count(&self) -> Result<u64, ProviderError>;
    async fn peers(&self) -> Result<Vec<crate::rpc::evm_rpc::PeerInfo>, ProviderError>;
    async fn add_peer(&self, req: crate::rpc::evm_rpc::NetAddPeerRequest) -> Result<(), ProviderError>;
    async fn node_info(&self) -> Result<crate::rpc::evm_rpc::NetNodeInfoResponse, ProviderError>;
}

pub trait DomainBlockchainProvider: EthReadProvider + EthWriteProvider + EngineProvider + NetProvider {}
impl<T> DomainBlockchainProvider for T where T: EthReadProvider + EthWriteProvider + EngineProvider + NetProvider {}
