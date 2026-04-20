use alloy_rpc_types::{SyncStatus, TransactionRequest};
use alloy_primitives::{U256, Address, B256, Bytes};
use alloy_eips::BlockId;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::info;
use crate::rpc::parse_address;
use crate::rpc::eth_service::EthService;

use crate::misc::error::RpcResult;

#[rpc(server)]
pub trait EthRpc {
    #[method(name = "eth_gasPrice")]
    async fn gas_price(&self) -> RpcResult<U256>;
    #[method(name = "eth_accounts")]
    async fn accounts(&self) -> RpcResult<Vec<Address>>;
    #[method(name = "eth_syncing")]
    async fn syncing(&self) -> RpcResult<SyncStatus>;
    #[method(name = "eth_mining")]
    async fn mining(&self) -> RpcResult<bool>;
    #[method(name = "eth_sendTransaction")]
    async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<B256>;
    #[method(name = "eth_sendRawTransaction")]
    async fn send_raw_transaction(&self, data: String) -> RpcResult<B256>;
    #[method(name = "eth_signTransaction")]
    async fn sign_transaction(&self, request: TransactionRequest) -> RpcResult<Bytes>;
    #[method(name = "eth_sign")]
    async fn sign(&self, address: String, message: String) -> RpcResult<Bytes>;
    #[method(name = "eth_call")]
    async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<Bytes>;
    #[method(name = "eth_estimateGas")]
    async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<U256>;
}

pub struct EthController {
    pub service: EthService,
}

#[async_trait]
impl EthRpcServer for EthController {
    async fn gas_price(&self) -> RpcResult<U256> {
        info!("[RPC] eth_gasPrice");
        let price = self.service.gas_price().await?;
        info!("[RPC] eth_gasPrice result: {}", price);
        Ok(price)
    }

    async fn accounts(&self) -> RpcResult<Vec<Address>> {
        info!("[RPC] eth_accounts");
        let accounts = self.service.accounts().await?;
        info!("[RPC] eth_accounts result count: {}", accounts.len());
        Ok(accounts)
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

    async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<B256> {
        info!("[RPC] eth_sendTransaction: request={:?}", request);
        let hash = self.service.send_transaction(request).await?;
        info!("[RPC] eth_sendTransaction result: {}", hash);
        Ok(hash)
    }

    async fn send_raw_transaction(&self, data: String) -> RpcResult<B256> {
        info!("[RPC] eth_sendRawTransaction: data_len={}", data.len());
        let hash = self.service.send_raw_transaction(data).await?;
        info!("[RPC] eth_sendRawTransaction result: {}", hash);
        Ok(hash)
    }

    async fn sign_transaction(&self, request: TransactionRequest) -> RpcResult<Bytes> {
        info!("[RPC] eth_signTransaction: request={:?}", request);
        let signed_tx_rlp = self.service.sign_transaction(request).await?;
        info!("[RPC] eth_signTransaction result size: {}", signed_tx_rlp.len());
        Ok(signed_tx_rlp)
    }

    async fn sign(&self, address: String, message: String) -> RpcResult<Bytes> {
        info!("[RPC] eth_sign: address={}, message_len={}", address, message.len());
        let addr = parse_address(&address)?;
        let signature = self.service.sign(addr, message).await?;
        let result = Bytes::from(signature.as_bytes().to_vec());
        info!("[RPC] eth_sign result size: {}", result.len());
        Ok(result)
    }

    async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<Bytes> {
        info!("[RPC] eth_call: request={:?}, block_id={:?}", request, block_id);
        let result_bytes = self.service.call(request, block_id).await?;
        info!("[RPC] eth_call result size: {}", result_bytes.len());
        Ok(result_bytes)
    }

    async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<U256> {
        info!("[RPC] eth_estimateGas: request={:?}, block_id={:?}", request, block_id);
        let gas = self.service.estimate_gas(request, block_id).await?;
        info!("[RPC] eth_estimateGas result: {}", gas);
        Ok(gas)
    }
}
