use std::sync::Arc;
use tokio::sync::RwLock;
use alloy_consensus::TxEnvelope as Transaction;
use crate::misc::error::RpcResult;
use crate::storage::mempool::Mempool;

pub struct DebugService {
    pub mempool: Arc<RwLock<Mempool>>,
}

impl DebugService {
    pub async fn get_mempool(&self) -> RpcResult<Vec<Transaction>> {
        Ok(self.mempool.read().await.get_all_transactions())
    }
}