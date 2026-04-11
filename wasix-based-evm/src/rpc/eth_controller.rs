use crate::error::RpcResult;
use alloy_rpc_types::{SyncStatus, TransactionRequest};
use alloy_eips::BlockId;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::eth_service::EthService;
use crate::rpc::account_mapper::AccountMapper;

#[rpc(server)]
pub trait EthRpc {
    #[method(name = "eth_gasPrice")]
    async fn gas_price(&self) -> RpcResult<String>;
    #[method(name = "eth_accounts")]
    async fn accounts(&self) -> RpcResult<Vec<String>>;
    #[method(name = "eth_syncing")]
    async fn syncing(&self) -> RpcResult<jsonrpsee::core::RpcResult<SyncStatus>>;
    #[method(name = "eth_mining")]
    async fn mining(&self) -> RpcResult<bool>;
    #[method(name = "eth_sendTransaction")]
    async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<String>;
    #[method(name = "eth_call")]
    async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<String>;
    #[method(name = "eth_estimateGas")]
    async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<String>;
}

pub struct EthController {
    pub service: EthService,
}

#[async_trait]
impl EthRpcServer for EthController {
    async fn gas_price(&self) -> RpcResult<String> {
        let price = self.service.gas_price().await?;
        Ok(format!("0x{:x}", price))
    }

    async fn accounts(&self) -> RpcResult<Vec<String>> {
        let accounts = self.service.accounts().await?;
        Ok(AccountMapper::addresses_to_rpc(accounts))
    }

    async fn syncing(&self) -> RpcResult<jsonrpsee::core::RpcResult<SyncStatus>> {
        let status = self.service.syncing().await?;
        Ok(Ok(status))
    }

    async fn mining(&self) -> RpcResult<bool> {
        Ok(false)
    }

    async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<String> {
        let hash = self.service.send_transaction(request).await?;
        Ok(format!("0x{:x}", hash))
    }

    async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<String> {
        let result = self.service.call(request, block_id).await?;
        Ok(format!("0x{:x}", result))
    }

    async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<String> {
        let gas = self.service.estimate_gas(request, block_id).await?;
        Ok(format!("0x{:x}", gas))
    }
}
