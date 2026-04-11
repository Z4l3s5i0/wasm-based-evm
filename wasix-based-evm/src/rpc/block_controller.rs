use crate::error::RpcResult;
use alloy_eips::BlockId;
use alloy_rpc_types::Block;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
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
}

pub struct BlockController {
    pub service: BlockService,
}

#[async_trait]
impl BlockRpcServer for BlockController {
    async fn get_block_by_number(&self, num: BlockId, full: bool) -> RpcResult<Option<Block>> {
        self.service.get_block_by_id(num, full).await
    }

    async fn get_block_by_hash(&self, hash: String, full: bool) -> RpcResult<Option<Block>> {
        let hash = parse_b256(&hash)?;
        self.service.get_block_by_id(BlockId::hash(hash), full).await
    }

    async fn block_number(&self) -> RpcResult<String> {
        let num = self.service.latest_block_number().await?;
        Ok(format!("0x{:x}", num))
    }
}