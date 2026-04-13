use crate::error::RpcResult;
use alloy_rpc_types::Transaction;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::debug_service::DebugService;
use crate::rpc::transaction_mapper::TransactionMapper;

#[rpc(server)]
pub trait DebugRpc {
    #[method(name = "debug_getMempool")]
    async fn get_mempool(&self) -> RpcResult<Vec<Transaction>>;
}

pub struct DebugController {
    pub service: DebugService,
}

#[async_trait]
impl DebugRpcServer for DebugController {
    async fn get_mempool(&self) -> RpcResult<Vec<Transaction>> {
        let mempool_txs = self.service.get_mempool().await?;
        Ok(mempool_txs.into_iter().map(|tx| TransactionMapper::to_rpc_transaction(tx, None)).collect())
    }
}