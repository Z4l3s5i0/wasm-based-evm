use crate::error::RpcResult;
use std::sync::Arc;
use alloy_eips::BlockId;
use alloy_rpc_types::Block;
use crate::storage::traits::BlockProvider;
use crate::rpc::block_mapper::BlockMapper;

pub struct BlockService {
    pub storage: Arc<dyn BlockProvider>,
}

impl BlockService {
    pub async fn get_block_by_id(&self, id: BlockId, full: bool) -> RpcResult<Option<Block>> {
        let block = self.storage.block(id).await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?;
        Ok(block.map(|b| BlockMapper::to_rpc_block(b, full)))
    }

    pub async fn latest_block_number(&self) -> RpcResult<u64> {
        self.storage.latest_block_number().await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))
    }

    pub async fn chain_id(&self) -> RpcResult<u64> {
        self.storage.chain_id().await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))
    }

    pub async fn get_block_transaction_count(&self, id: BlockId) -> RpcResult<Option<u64>> {
        let block = self.storage.block(id).await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?;
        Ok(block.map(|b| b.body.transactions.len() as u64))
    }
}