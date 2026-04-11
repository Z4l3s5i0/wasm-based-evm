use crate::error::RpcResult;
use alloy_eips::BlockId;
use alloy_primitives::U256;
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
}

pub struct AccountController {
    pub service: AccountService,
}

#[async_trait]
impl AccountRpcServer for AccountController {
    async fn get_balance(&self, address: String, block_id: Option<BlockId>) -> RpcResult<String> {
        let addr = parse_address(&address)?;
        let block_id = block_id.unwrap_or(BlockId::latest());
        let balance = self.service.get_balance(addr, block_id).await?;
        Ok(AccountMapper::to_hex(balance))
    }

    async fn get_transaction_count(&self, address: String, block_id: Option<BlockId>) -> RpcResult<String> {
        let addr = parse_address(&address)?;
        let block_id = block_id.unwrap_or(BlockId::latest());
        let count = self.service.get_transaction_count(addr, block_id).await?;
        Ok(AccountMapper::to_hex(U256::from(count)))
    }
    
}