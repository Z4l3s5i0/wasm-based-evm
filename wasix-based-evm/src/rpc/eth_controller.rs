use crate::{info, error};
use crate::error::RpcResult;
use alloy_rpc_types::{SyncStatus, TransactionRequest};
use alloy_eips::BlockId;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::parse_address;
use crate::rpc::eth_service::EthService;
use crate::rpc::account_mapper::AccountMapper;

#[rpc(server)]
pub trait EthRpc {
    #[method(name = "eth_gasPrice")]
    async fn gas_price(&self) -> RpcResult<String>;
    #[method(name = "eth_accounts")]
    async fn accounts(&self) -> RpcResult<Vec<String>>;
    #[method(name = "eth_syncing")]
    async fn syncing(&self) -> RpcResult<SyncStatus>;
    #[method(name = "eth_mining")]
    async fn mining(&self) -> RpcResult<bool>;
    #[method(name = "eth_sendTransaction")]
    async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<String>;
    #[method(name = "eth_sendRawTransaction")]
    async fn send_raw_transaction(&self, data: String) -> RpcResult<String>;
    #[method(name = "eth_signTransaction")]
    async fn sign_transaction(&self, request: TransactionRequest) -> RpcResult<String>;
    #[method(name = "eth_sign")]
    async fn sign(&self, address: String, message: String) -> RpcResult<String>;
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
        info!("[RPC] eth_gasPrice");
        let price = self.service.gas_price().await?;
        let result = format!("0x{:x}", price);
        info!("[RPC] eth_gasPrice result: {}", result);
        Ok(result)
    }

    async fn accounts(&self) -> RpcResult<Vec<String>> {
        info!("[RPC] eth_accounts");
        let accounts = self.service.accounts().await?;
        let result = AccountMapper::addresses_to_rpc(accounts);
        info!("[RPC] eth_accounts result count: {}", result.len());
        Ok(result)
    }

    async fn syncing(&self) -> RpcResult<SyncStatus> {
        info!("[RPC] eth_syncing");
        let status = self.service.syncing().await?;
        info!("[RPC] eth_syncing result: {:?}", status);
        Ok(status)
    }

    async fn mining(&self) -> RpcResult<bool> {
        info!("[RPC] eth_mining");
        Ok(false)
    }

    async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<String> {
        info!("[RPC] eth_sendTransaction: request={:?}", request);
        let hash = self.service.send_transaction(request).await?;
        let result = format!("0x{:x}", hash);
        info!("[RPC] eth_sendTransaction result: {}", result);
        Ok(result)
    }

    async fn send_raw_transaction(&self, data: String) -> RpcResult<String> {
        info!("[RPC] eth_sendRawTransaction: data_len={}", data.len());
        let hash = self.service.send_raw_transaction(data).await?;
        let result = format!("0x{:x}", hash);
        info!("[RPC] eth_sendRawTransaction result: {}", result);
        Ok(result)
    }

    async fn sign_transaction(&self, request: TransactionRequest) -> RpcResult<String> {
        info!("[RPC] eth_signTransaction: request={:?}", request);
        let signed_tx_rlp = self.service.sign_transaction(request).await?;
        let result = format!("{}", signed_tx_rlp);
        info!("[RPC] eth_signTransaction result size: {}", result.len());
        Ok(result)
    }

    async fn sign(&self, address: String, message: String) -> RpcResult<String> {
        info!("[RPC] eth_sign: address={}, message_len={}", address, message.len());
        let addr = parse_address(&address)?;
        let signature = self.service.sign(addr, message).await?;
        let result = format!("0x{}", hex::encode(signature.as_bytes()));
        info!("[RPC] eth_sign result size: {}", result.len());
        Ok(result)
    }

    async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<String> {
        info!("[RPC] eth_call: request={:?}, block_id={:?}", request, block_id);
        let result_bytes = self.service.call(request, block_id).await?;
        let result = format!("0x{:x}", result_bytes);
        info!("[RPC] eth_call result: {}", result);
        Ok(result)
    }

    async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<String> {
        info!("[RPC] eth_estimateGas: request={:?}, block_id={:?}", request, block_id);
        let gas = self.service.estimate_gas(request, block_id).await?;
        let result = format!("0x{:x}", gas);
        info!("[RPC] eth_estimateGas result: {}", result);
        Ok(result)
    }
}
