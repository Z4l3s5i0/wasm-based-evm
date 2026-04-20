use crate::info;
use alloy_eips::BlockId;
use alloy_rpc_types::Block;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::misc::error::RpcResult;
use crate::rpc::block_service::BlockService;
use crate::rpc::parse_b256;

#[rpc(server)]
pub trait BlockRpc {
    #[method(name = "eth_getBlockByNumber")]
    async fn get_block_by_number(&self, num: BlockId, full: bool) -> RpcResult<Option<Block>>;
    #[method(name = "eth_getBlockByHash")]
    async fn get_block_by_hash(&self, hash: String, full: bool) -> RpcResult<Option<Block>>;
    #[method(name = "eth_blockNumber")]
    async fn block_number(&self) -> RpcResult<String>;
    #[method(name = "eth_chainId")]
    async fn chain_id(&self) -> RpcResult<String>;
    #[method(name = "eth_getBlockTransactionCountByNumber")]
    async fn get_block_transaction_count_by_number(&self, num: BlockId) -> RpcResult<Option<String>>;
    #[method(name = "eth_getBlockTransactionCountByHash")]
    async fn get_block_transaction_count_by_hash(&self, hash: String) -> RpcResult<Option<String>>;
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

    async fn get_block_by_hash(&self, hash: String, full: bool) -> RpcResult<Option<Block>> {
        info!("[RPC] eth_getBlockByHash: hash={}, full={}", hash, full);
        let hash_b256 = parse_b256(&hash)?;
        let result = self.service.get_block_by_id(BlockId::hash(hash_b256), full).await?;
        info!("[RPC] eth_getBlockByHash result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn block_number(&self) -> RpcResult<String> {
        info!("[RPC] eth_blockNumber");
        let num = self.service.latest_block_number().await?;
        let result = format!("0x{:x}", num);
        info!("[RPC] eth_blockNumber result: {}", result);
        Ok(result)
    }

    async fn chain_id(&self) -> RpcResult<String> {
        info!("[RPC] eth_chainId");
        let id = self.service.chain_id().await?;
        let result = format!("0x{:x}", id);
        info!("[RPC] eth_chainId result: {}", result);
        Ok(result)
    }

    async fn get_block_transaction_count_by_number(&self, num: BlockId) -> RpcResult<Option<String>> {
        info!("[RPC] eth_getBlockTransactionCountByNumber: num={:?}", num);
        let count = self.service.get_block_transaction_count(num).await?;
        let result = count.map(|c| format!("0x{:x}", c));
        info!("[RPC] eth_getBlockTransactionCountByNumber result: {:?}", result);
        Ok(result)
    }

    async fn get_block_transaction_count_by_hash(&self, hash: String) -> RpcResult<Option<String>> {
        info!("[RPC] eth_getBlockTransactionCountByHash: hash={}", hash);
        let hash_b256 = parse_b256(&hash)?;
        let count = self.service.get_block_transaction_count(BlockId::hash(hash_b256)).await?;
        let result = count.map(|c| format!("0x{:x}", c));
        info!("[RPC] eth_getBlockTransactionCountByHash result: {:?}", result);
        Ok(result)
    }
}