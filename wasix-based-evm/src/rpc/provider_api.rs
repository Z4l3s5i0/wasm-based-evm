use alloy_primitives::{Address, B256};
use crate::storage::types::{Block, Receipt, Transaction, ExecutionPayload as DomainExecutionPayload};
use crate::rpc::provider_error::ProviderError;

#[derive(Debug, Clone)]
pub struct DomainTransactionRequest {
    pub from: Address,
    pub to: Option<Address>,
    pub value: alloy_primitives::U256,
    pub nonce: u64,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: alloy_primitives::U256,
}

#[derive(Debug, Clone)]
pub struct DomainProposeBlockRequest {
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct DomainPayloadStatus {
    pub status: String,
    pub latest_valid_hash: Option<B256>,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DomainPayloadAttributes {
    pub timestamp: u64,
    pub prev_randao: B256,
    pub suggested_fee_recipient: Address,
    pub withdrawals: Vec<crate::storage::types::Withdrawal>,
    pub parent_beacon_block_root: Option<B256>,
}

#[derive(Debug, Clone)]
pub struct DomainForkchoiceUpdatedRequest {
    pub head_block_hash: B256,
    pub safe_block_hash: B256,
    pub finalized_block_hash: B256,
    pub payload_attributes: Option<DomainPayloadAttributes>,
}

#[derive(Debug, Clone)]
pub struct DomainForkchoiceUpdatedResponse {
    pub status: String,
    pub payload_id: Option<B256>,
}

#[derive(Debug, Clone)]
pub struct DomainGetPayloadRequest {
    pub payload_id: B256,
}

#[derive(Debug, Clone)]
pub struct DomainPeerInfo {
    pub id: String,
    pub enode: String,
    pub enr: String,
    pub name: String,
    pub caps: Vec<String>,
    pub network: DomainPeerNetworkInfo,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DomainPeerNetworkInfo {
    pub local_address: String,
    pub remote_address: String,
    pub inbound: bool,
    pub trusted: bool,
    pub static_node: bool,
}

#[derive(Debug, Clone)]
pub struct DomainNetAddPeerRequest {
    pub enode: String,
}

#[derive(Debug, Clone)]
pub struct DomainNetNodeInfoResponse {
    pub enode: String,
    pub enr: String,
    pub name: String,
    pub caps: Vec<String>,
    pub id: String,
    pub network: DomainNodeNetworkInfo,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DomainNodeNetworkInfo {
    pub local_address: String,
    pub remote_address: String,
    pub listen_addr: String,
}

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
    async fn send_transaction(&self, req: DomainTransactionRequest) -> Result<Transaction, ProviderError>;
    async fn call(&self, req: DomainTransactionRequest) -> Result<Transaction, ProviderError>;
}

#[async_trait::async_trait]
pub trait EngineProvider: Send + Sync {
    async fn propose_block(&self, req: DomainProposeBlockRequest) -> Result<ProposeBlockResult, ProviderError>;
    async fn engine_new_payload(&self, payload: DomainExecutionPayload) -> Result<DomainPayloadStatus, ProviderError>;
    async fn engine_forkchoice_updated(&self, req: DomainForkchoiceUpdatedRequest) -> Result<DomainForkchoiceUpdatedResponse, ProviderError>;
    async fn engine_get_payload(&self, req: DomainGetPayloadRequest) -> Result<DomainExecutionPayload, ProviderError>;
}

pub struct ProposeBlockResult {
    pub block_hash: B256,
    pub tx_results: Vec<Transaction>,
}

#[async_trait::async_trait]
pub trait NetProvider: Send + Sync {
    async fn peer_count(&self) -> Result<u64, ProviderError>;
    async fn peers(&self) -> Result<Vec<DomainPeerInfo>, ProviderError>;
    async fn add_peer(&self, req: DomainNetAddPeerRequest) -> Result<(), ProviderError>;
    async fn node_info(&self) -> Result<DomainNetNodeInfoResponse, ProviderError>;
}

pub trait DomainBlockchainProvider: EthReadProvider + EthWriteProvider + EngineProvider + NetProvider {}
impl<T> DomainBlockchainProvider for T where T: EthReadProvider + EthWriteProvider + EngineProvider + NetProvider {}
