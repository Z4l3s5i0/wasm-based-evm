use crate::error::RpcResult;
use alloy_primitives::B256;
use alloy_rpc_types::Transaction;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::transaction_service::TransactionService;
use crate::rpc::parse_b256;

#[rpc(server)]
pub trait TransactionRpc {
    #[method(name = "eth_getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<Transaction>>;
    #[method(name = "eth_getTransactionReceipt")]
    async fn get_transaction_receipt(&self, hash: String) -> RpcResult<Option<alloy_rpc_types::TransactionReceipt>>;
}

pub struct TransactionController {
    pub service: TransactionService,
}

#[async_trait]
impl TransactionRpcServer for TransactionController {
    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<Transaction>> {
        let hash = parse_b256(&hash)?;
        self.service.get_transaction_by_hash(hash).await
    }

    async fn get_transaction_receipt(&self, hash: String) -> RpcResult<Option<alloy_rpc_types::TransactionReceipt>> {
        let hash = parse_b256(&hash)?;
        self.service.get_transaction_receipt(hash).await
    }
}
