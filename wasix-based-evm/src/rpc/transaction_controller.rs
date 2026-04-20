use crate::info;
use crate::misc::error::RpcResult;
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
        info!("[RPC] eth_getTransactionByHash: hash={}", hash);
        let hash_b256 = parse_b256(&hash)?;
        let result = self.service.get_transaction_by_hash(hash_b256).await?;
        info!("[RPC] eth_getTransactionByHash result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn get_transaction_receipt(&self, hash: String) -> RpcResult<Option<alloy_rpc_types::TransactionReceipt>> {
        info!("[RPC] eth_getTransactionReceipt: hash={}", hash);
        let hash_b256 = parse_b256(&hash)?;
        let result = self.service.get_transaction_receipt(hash_b256).await?;
        info!("[RPC] eth_getTransactionReceipt result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }
}
