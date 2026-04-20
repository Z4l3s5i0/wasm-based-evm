use crate::info;
use crate::error::RpcResult;
use alloy_eips::BlockId;
use alloy_primitives::{U256, Bytes, B256};
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::account_mapper::AccountMapper;
use crate::rpc::account_service::AccountService;
use crate::rpc::parse_address;

#[rpc(server)]
pub trait AccountRpc {
    #[method(name = "eth_getBalance")]
    async fn get_balance(&self, address: String, block_id: Option<BlockId>) -> RpcResult<String>;
    #[method(name = "eth_getTransactionCount")]
    async fn get_transaction_count(&self, address: String, block_id: Option<BlockId>) -> RpcResult<String>;
    #[method(name = "eth_getCode")]
    async fn get_code(&self, address: String, block_id: Option<BlockId>) -> RpcResult<Bytes>;
    #[method(name = "eth_getStorageAt")]
    async fn get_storage_at(&self, address: String, slot: String, block_id: Option<BlockId>) -> RpcResult<String>;
}

pub struct AccountController {
    pub service: AccountService,
}

#[async_trait]
impl AccountRpcServer for AccountController {
    async fn get_balance(&self, address: String, block_id: Option<BlockId>) -> RpcResult<String> {
        info!("[RPC] eth_getBalance: address={}, block_id={:?}", address, block_id);
        let addr = parse_address(&address)?;
        let block_id = block_id.unwrap_or(BlockId::latest());
        let balance = self.service.get_balance(addr, block_id).await?;
        let result = AccountMapper::to_hex(balance);
        info!("[RPC] eth_getBalance result: {}", result);
        Ok(result)
    }

    async fn get_transaction_count(&self, address: String, block_id: Option<BlockId>) -> RpcResult<String> {
        info!("[RPC] eth_getTransactionCount: address={}, block_id={:?}", address, block_id);
        let addr = parse_address(&address)?;
        let block_id = block_id.unwrap_or(BlockId::latest());
        let count = self.service.get_transaction_count(addr, block_id).await?;
        let result = AccountMapper::to_hex(U256::from(count));
        info!("[RPC] eth_getTransactionCount result: {}", result);
        Ok(result)
    }

    async fn get_code(&self, address: String, block_id: Option<BlockId>) -> RpcResult<Bytes> {
        info!("[RPC] eth_getCode: address={}, block_id={:?}", address, block_id);
        let addr = parse_address(&address)?;
        let block_id = block_id.unwrap_or(BlockId::latest());
        let result = self.service.get_code(addr, block_id).await?;
        info!("[RPC] eth_getCode result size: {}", result.len());
        Ok(result)
    }

    async fn get_storage_at(&self, address: String, slot: String, block_id: Option<BlockId>) -> RpcResult<String> {
        info!("[RPC] eth_getStorageAt: address={}, slot={}, block_id={:?}", address, slot, block_id);
        let addr = parse_address(&address)?;
        let slot = slot.parse::<B256>().map_err(|e| crate::error::RpcError::InvalidParams(format!("Invalid slot: {}", e)))?;
        let block_id = block_id.unwrap_or(BlockId::latest());
        let value = self.service.get_storage_at(addr, slot, block_id).await?;
        let result = AccountMapper::to_hex(value);
        info!("[RPC] eth_getStorageAt result: {}", result);
        Ok(result)
    }
}