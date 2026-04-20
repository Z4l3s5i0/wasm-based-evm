use alloy_primitives::B256;
use alloy_rpc_types::{Transaction, TransactionReceipt};
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::info;
use crate::misc::error::RpcResult;
use crate::rpc::transaction_service::TransactionService;

#[rpc(server)]
pub trait TransactionRpc {
    #[method(name = "eth_getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: B256) -> RpcResult<Option<Transaction>>;
    #[method(name = "eth_getTransactionReceipt")]
    async fn get_transaction_receipt(&self, hash: B256) -> RpcResult<Option<TransactionReceipt>>;
}

pub struct TransactionController {
    pub service: TransactionService,
}

#[async_trait]
impl TransactionRpcServer for TransactionController {
    async fn get_transaction_by_hash(&self, hash: B256) -> RpcResult<Option<Transaction>> {
        info!("[RPC] eth_getTransactionByHash: hash={}", hash);
        let result = self.service.get_transaction_by_hash(hash).await?;
        info!("[RPC] eth_getTransactionByHash result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn get_transaction_receipt(&self, hash: B256) -> RpcResult<Option<TransactionReceipt>> {
        info!("[RPC] eth_getTransactionReceipt: hash={}", hash);
        let result = self.service.get_transaction_receipt(hash).await?;
        info!("[RPC] eth_getTransactionReceipt result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }
}
