use alloy_eips::BlockId;
use alloy_primitives::{U256, Bytes, B256, Address};
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::info;
use crate::misc::error::RpcResult;
use crate::rpc::account_service::AccountService;

#[rpc(server)]
pub trait AccountRpc {
    #[method(name = "eth_getBalance")]
    async fn get_balance(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<U256>;
    #[method(name = "eth_getTransactionCount")]
    async fn get_transaction_count(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<u64>;
    #[method(name = "eth_getCode")]
    async fn get_code(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<Bytes>;
    #[method(name = "eth_getStorageAt")]
    async fn get_storage_at(&self, address: Address, slot: B256, block_id: Option<BlockId>) -> RpcResult<U256>;
}

pub struct AccountController {
    pub service: AccountService,
}

#[async_trait]
impl AccountRpcServer for AccountController {
    async fn get_balance(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<U256> {
        info!("[RPC] eth_getBalance: address={}, block_id={:?}", address, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let balance = self.service.get_balance(address, block_id).await?;
        info!("[RPC] eth_getBalance result: {}", balance);
        Ok(balance)
    }

    async fn get_transaction_count(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<u64> {
        info!("[RPC] eth_getTransactionCount: address={}, block_id={:?}", address, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let count = self.service.get_transaction_count(address, block_id).await?;
        info!("[RPC] eth_getTransactionCount result: {}", count);
        Ok(count)
    }

    async fn get_code(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<Bytes> {
        info!("[RPC] eth_getCode: address={}, block_id={:?}", address, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let result = self.service.get_code(address, block_id).await?;
        info!("[RPC] eth_getCode result size: {}", result.len());
        Ok(result)
    }

    async fn get_storage_at(&self, address: Address, slot: B256, block_id: Option<BlockId>) -> RpcResult<U256> {
        info!("[RPC] eth_getStorageAt: address={}, slot={}, block_id={:?}", address, slot, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let value = self.service.get_storage_at(address, slot, block_id).await?;
        info!("[RPC] eth_getStorageAt result: {}", value);
        Ok(value)
    }
}