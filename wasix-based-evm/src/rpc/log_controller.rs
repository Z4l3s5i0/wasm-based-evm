use crate::{info, error};
use crate::error::RpcResult;
use alloy_rpc_types::{Filter, Log};
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::log_service::LogService;

#[rpc(server)]
pub trait LogRpc {
    #[method(name = "eth_getLogs")]
    async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>>;
}

pub struct LogController {
    pub service: LogService,
}

#[async_trait]
impl LogRpcServer for LogController {
    async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>> {
        info!("[RPC] eth_getLogs: filter={:?}", filter);
        let result = self.service.get_logs(filter).await?;
        info!("[RPC] eth_getLogs result count: {}", result.len());
        Ok(result)
    }
}
