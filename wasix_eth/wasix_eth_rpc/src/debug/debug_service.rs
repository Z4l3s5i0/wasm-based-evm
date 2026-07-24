use std::sync::Arc;
use alloy_rlp::Encodable;
use wasix_eth_core::mempool::mempool_provider::MempoolProvider;
use wasix_eth_core::engine::api::RPCEngine;
use wasix_eth_types::{BlockId, B256, Bytes, Transaction};
use wasix_eth_types::error::{RpcError, RpcResult};

pub struct DebugService {
    pub mempool: Arc<dyn MempoolProvider>,
    pub engine: Arc<RPCEngine>,
}

impl DebugService {
    pub fn new(mempool: Arc<dyn MempoolProvider>, engine: Arc<RPCEngine>) -> Self {
        Self { mempool, engine }
    }

    pub async fn get_mempool(&self) -> RpcResult<Vec<Transaction>> {
        Ok(self.mempool.get_all_transactions().await)
    }

    pub async fn get_raw_block(&self, id: BlockId) -> RpcResult<Bytes> {
        let block = self.engine.get_block_by_id(id).await?
            .ok_or_else(|| RpcError::Internal("Block not found".to_string()))?;
        let mut buf = Vec::new();
        block.encode(&mut buf);
        Ok(Bytes::from(buf))
    }

    pub async fn get_raw_header(&self, id: BlockId) -> RpcResult<Bytes> {
        let block = self.engine.get_block_by_id(id).await?
            .ok_or_else(|| RpcError::Internal("Block not found".to_string()))?;
        let mut buf = Vec::new();
        block.header.encode(&mut buf);
        Ok(Bytes::from(buf))
    }

    pub async fn get_raw_receipts(&self, id: BlockId) -> RpcResult<Vec<Bytes>> {
        let receipts = self.engine.get_receipts_by_block_id(id).await?;

        let mut result = Vec::new();
        for receipt in receipts {
            let json = serde_json::to_vec(&receipt).map_err(|e| RpcError::Internal(e.to_string()))?;
            result.push(Bytes::from(json));
        }
        Ok(result)
    }

    pub async fn get_raw_transaction(&self, hash: B256) -> RpcResult<Bytes> {
        let (tx, _) = self.engine.get_transaction_by_hash(hash).await?
            .ok_or_else(|| RpcError::Internal("Transaction not found".to_string()))?;
        let mut buf = Vec::new();
        tx.encode(&mut buf);
        Ok(Bytes::from(buf))
    }
}