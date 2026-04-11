use crate::error::RpcResult;
use alloy_rpc_types::{Filter, Log};
use std::sync::Arc;
use crate::storage::traits::LogProvider;

pub struct LogService {
    pub storage: Arc<dyn LogProvider>,
}

impl LogService {
    pub async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>> {
        self.storage.logs(filter).await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))
    }
}
