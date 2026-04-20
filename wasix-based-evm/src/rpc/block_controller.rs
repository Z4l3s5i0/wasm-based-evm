use alloy_eips::BlockId;
use alloy_rpc_types::Block;
use alloy_primitives::B256;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::info;
use crate::misc::error::RpcResult;
use crate::rpc::block_service::BlockService;

#[rpc(server)]
pub trait BlockRpc {
    #[method(name = "eth_getBlockByNumber")]
    async fn get_block_by_number(&self, num: BlockId, full: bool) -> RpcResult<Option<Block>>;
    #[method(name = "eth_getBlockByHash")]
    async fn get_block_by_hash(&self, hash: B256, full: bool) -> RpcResult<Option<Block>>;
    #[method(name = "eth_blockNumber")]
    async fn block_number(&self) -> RpcResult<u64>;
    #[method(name = "eth_chainId")]
    async fn chain_id(&self) -> RpcResult<u64>;
    #[method(name = "eth_getBlockTransactionCountByNumber")]
    async fn get_block_transaction_count_by_number(&self, num: BlockId) -> RpcResult<Option<u64>>;
    #[method(name = "eth_getBlockTransactionCountByHash")]
    async fn get_block_transaction_count_by_hash(&self, hash: B256) -> RpcResult<Option<u64>>;
}

pub struct BlockController {
    pub service: BlockService,
}

#[async_trait]
impl BlockRpcServer for BlockController {
    async fn get_block_by_number(&self, num: BlockId, full: bool) -> RpcResult<Option<Block>> {
        info!("[RPC] eth_getBlockByNumber: num={:?}, full={}", num, full);
        let result = self.service.get_block_by_id(num, full).await?;
        info!("[RPC] eth_getBlockByNumber result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn get_block_by_hash(&self, hash: B256, full: bool) -> RpcResult<Option<Block>> {
        info!("[RPC] eth_getBlockByHash: hash={}, full={}", hash, full);
        let result = self.service.get_block_by_id(BlockId::hash(hash), full).await?;
        info!("[RPC] eth_getBlockByHash result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn block_number(&self) -> RpcResult<u64> {
        info!("[RPC] eth_blockNumber");
        let num = self.service.latest_block_number().await?;
        info!("[RPC] eth_blockNumber result: {}", num);
        Ok(num)
    }

    async fn chain_id(&self) -> RpcResult<u64> {
        info!("[RPC] eth_chainId");
        let id = self.service.chain_id().await?;
        info!("[RPC] eth_chainId result: {}", id);
        Ok(id)
    }

    async fn get_block_transaction_count_by_number(&self, num: BlockId) -> RpcResult<Option<u64>> {
        info!("[RPC] eth_getBlockTransactionCountByNumber: num={:?}", num);
        let count = self.service.get_block_transaction_count(num).await?;
        info!("[RPC] eth_getBlockTransactionCountByNumber result: {:?}", count);
        Ok(count)
    }

    async fn get_block_transaction_count_by_hash(&self, hash: B256) -> RpcResult<Option<u64>> {
        info!("[RPC] eth_getBlockTransactionCountByHash: hash={}", hash);
        let count = self.service.get_block_transaction_count(BlockId::hash(hash)).await?;
        info!("[RPC] eth_getBlockTransactionCountByHash result: {:?}", count);
        Ok(count)
    }
}