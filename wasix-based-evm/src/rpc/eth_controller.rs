use alloy_rpc_types::{Block, Filter, Log, SyncStatus, Transaction, TransactionReceipt, TransactionRequest};
use alloy_primitives::{U256, Address, B256, Bytes};
use alloy_eips::BlockId;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::{info, debug};
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
    #[method(name = "eth_blockNumber")]
    async fn block_number(&self) -> RpcResult<U256>;
    #[method(name = "eth_call")]
    async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<Bytes>;
    #[method(name = "eth_estimateGas")]
    async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<U256>;
    #[method(name = "eth_getBlockByNumber")]
    async fn get_block_by_number(&self, num: BlockId, full: bool) -> RpcResult<Option<Block>>;
    #[method(name = "eth_getBlockByHash")]
    async fn get_block_by_hash(&self, hash: B256, full: bool) -> RpcResult<Option<Block>>;
    #[method(name = "eth_chainId")]
    async fn chain_id(&self) -> RpcResult<U256>;
    #[method(name = "eth_getBlockTransactionCountByNumber")]
    async fn get_block_transaction_count_by_number(&self, num: BlockId) -> RpcResult<Option<U256>>;
    #[method(name = "eth_getBlockTransactionCountByHash")]
    async fn get_block_transaction_count_by_hash(&self, hash: B256) -> RpcResult<Option<U256>>;
    #[method(name = "eth_getLogs")]
    async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>>;
    #[method(name = "eth_getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: B256) -> RpcResult<Option<Transaction>>;
    #[method(name = "eth_getTransactionReceipt")]
    async fn get_transaction_receipt(&self, hash: B256) -> RpcResult<Option<TransactionReceipt>>;
    #[method(name = "eth_getBalance")]
    async fn get_balance(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<U256>;
    #[method(name = "eth_getTransactionCount")]
    async fn get_transaction_count(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<U256>;
    #[method(name = "eth_getCode")]
    async fn get_code(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<Bytes>;
    #[method(name = "eth_getStorageAt")]
    async fn get_storage_at(&self, address: Address, slot: B256, block_id: Option<BlockId>) -> RpcResult<U256>;
}

pub struct EthController {
    pub service: EthService,
}

#[async_trait]
impl EthRpcServer for EthController {
    async fn gas_price(&self) -> RpcResult<U256> {
        debug!("[RPC] eth_gasPrice");
        let price = self.service.gas_price().await?;
        debug!("[RPC] eth_gasPrice result: {}", price);
        Ok(price)
    }

    async fn accounts(&self) -> RpcResult<Vec<Address>> {
        debug!("[RPC] eth_accounts");
        let accounts = self.service.accounts().await?;
        debug!("[RPC] eth_accounts result count: {}", accounts.len());
        Ok(accounts)
    }

    async fn syncing(&self) -> RpcResult<SyncStatus> {
        debug!("[RPC] eth_syncing");
        let status = self.service.syncing().await?;
        debug!("[RPC] eth_syncing result: {:?}", status);
        Ok(status)
    }

    async fn mining(&self) -> RpcResult<bool> {
        debug!("[RPC] eth_mining");
        Ok(false)
    }

    async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<B256> {
        debug!("[RPC] eth_sendTransaction: request={:?}", request);
        let hash = self.service.send_transaction(request).await?;
        debug!("[RPC] eth_sendTransaction result: {}", hash);
        Ok(hash)
    }

    async fn send_raw_transaction(&self, data: String) -> RpcResult<B256> {
        debug!("[RPC] eth_sendRawTransaction: data_len={}", data.len());
        let hash = self.service.send_raw_transaction(data).await?;
        debug!("[RPC] eth_sendRawTransaction result: {}", hash);
        Ok(hash)
    }

    async fn sign_transaction(&self, request: TransactionRequest) -> RpcResult<Bytes> {
        debug!("[RPC] eth_signTransaction: request={:?}", request);
        let signed_tx_rlp = self.service.sign_transaction(request).await?;
        debug!("[RPC] eth_signTransaction result size: {}", signed_tx_rlp.len());
        Ok(signed_tx_rlp)
    }

    async fn sign(&self, address: String, message: String) -> RpcResult<Bytes> {
        debug!("[RPC] eth_sign: address={}, message_len={}", address, message.len());
        let addr = parse_address(&address)?;
        let signature = self.service.sign(addr, message).await?;
        let result = Bytes::from(signature.as_bytes().to_vec());
        debug!("[RPC] eth_sign result size: {}", result.len());
        Ok(result)
    }

    async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<Bytes> {
        debug!("[RPC] eth_call: request={:?}, block_id={:?}", request, block_id);
        let result_bytes = self.service.call(request, block_id).await?;
        debug!("[RPC] eth_call result size: {}", result_bytes.len());
        Ok(result_bytes)
    }

    async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<U256> {
        debug!("[RPC] eth_estimateGas: request={:?}, block_id={:?}", request, block_id);
        let gas = self.service.estimate_gas(request, block_id).await?;
        debug!("[RPC] eth_estimateGas result: {}", gas);
        Ok(gas)
    }


    async fn get_block_by_number(&self, num: BlockId, full: bool) -> RpcResult<Option<Block>> {
        debug!("[RPC] eth_getBlockByNumber: num={:?}, full={}", num, full);
        let result = self.service.get_block_by_id(num, full).await?;
        debug!("[RPC] eth_getBlockByNumber result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn get_block_by_hash(&self, hash: B256, full: bool) -> RpcResult<Option<Block>> {
        debug!("[RPC] eth_getBlockByHash: hash={}, full={}", hash, full);
        let result = self.service.get_block_by_id(BlockId::hash(hash), full).await?;
        debug!("[RPC] eth_getBlockByHash result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn block_number(&self) -> RpcResult<U256> {
        debug!("[RPC] eth_blockNumber");
        let num = self.service.latest_block_number().await?;
        debug!("[RPC] eth_blockNumber result: {}", num);
        Ok(U256::from(num))
    }

    async fn chain_id(&self) -> RpcResult<U256> {
        debug!("[RPC] eth_chainId");
        let id = self.service.chain_id().await?;
        debug!("[RPC] eth_chainId result: {}", id);
        Ok(U256::from(id))
    }

    async fn get_block_transaction_count_by_number(&self, num: BlockId) -> RpcResult<Option<U256>> {
        debug!("[RPC] eth_getBlockTransactionCountByNumber: num={:?}", num);
        let count = self.service.get_block_transaction_count(num).await?;
        debug!("[RPC] eth_getBlockTransactionCountByNumber result: {:?}", count);
        Ok(count.map(U256::from))
    }

    async fn get_block_transaction_count_by_hash(&self, hash: B256) -> RpcResult<Option<U256>> {
        debug!("[RPC] eth_getBlockTransactionCountByHash: hash={}", hash);
        let count = self.service.get_block_transaction_count(BlockId::hash(hash)).await?;
        debug!("[RPC] eth_getBlockTransactionCountByHash result: {:?}", count);
        Ok(count.map(U256::from))
    }

    async fn get_logs(&self, filter: Filter) -> RpcResult<Vec<Log>> {
        debug!("[RPC] eth_getLogs: filter={:?}", filter);
        let result = self.service.get_logs(filter).await?;
        debug!("[RPC] eth_getLogs result count: {}", result.len());
        Ok(result)
    }
    async fn get_transaction_by_hash(&self, hash: B256) -> RpcResult<Option<Transaction>> {
        debug!("[RPC] eth_getTransactionByHash: hash={}", hash);
        let result = self.service.get_transaction_by_hash(hash).await?;
        debug!("[RPC] eth_getTransactionByHash result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn get_transaction_receipt(&self, hash: B256) -> RpcResult<Option<TransactionReceipt>> {
        debug!("[RPC] eth_getTransactionReceipt: hash={}", hash);
        let result = self.service.get_transaction_receipt(hash).await?;
        debug!("[RPC] eth_getTransactionReceipt result: {}", if result.is_some() { "found" } else { "not found" });
        Ok(result)
    }

    async fn get_balance(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<U256> {
        debug!("[RPC] eth_getBalance: address={}, block_id={:?}", address, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let balance = self.service.get_balance(address, block_id).await?;
        debug!("[RPC] eth_getBalance result: {}", balance);
        Ok(balance)
    }

    async fn get_transaction_count(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<U256> {
        debug!("[RPC] eth_getTransactionCount: address={}, block_id={:?}", address, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let count = self.service.get_transaction_count(address, block_id).await?;
        debug!("[RPC] eth_getTransactionCount result: {}", count);
        Ok(U256::from(count))
    }

    async fn get_code(&self, address: Address, block_id: Option<BlockId>) -> RpcResult<Bytes> {
        debug!("[RPC] eth_getCode: address={}, block_id={:?}", address, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let result = self.service.get_code(address, block_id).await?;
        debug!("[RPC] eth_getCode result size: {}", result.len());
        Ok(result)
    }

    async fn get_storage_at(&self, address: Address, slot: B256, block_id: Option<BlockId>) -> RpcResult<U256> {
        debug!("[RPC] eth_getStorageAt: address={}, slot={}, block_id={:?}", address, slot, block_id);
        let block_id = block_id.unwrap_or(BlockId::latest());
        let value = self.service.get_storage_at(address, slot, block_id).await?;
        debug!("[RPC] eth_getStorageAt result: {}", value);
        Ok(value)
    }

}
