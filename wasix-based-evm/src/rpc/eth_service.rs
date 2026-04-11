use std::sync::Arc;
use crate::error::RpcResult;
use alloy_primitives::{U256, Address, Bytes};
use alloy_rpc_types::{SyncStatus, TransactionRequest};
use crate::storage::traits::{BlockProvider, StateProvider};
use alloy_eips::{BlockId, BlockNumberOrTag};
use tokio::sync::RwLock;
use crate::mempool::Mempool;
use crate::rpc::account_manager::AccountManager;
use crate::executor::Executor;
use alloy_consensus::{TxEnvelope as Transaction, TxLegacy};
use crate::storage::storage::InMemoryStorage;
use evm::standard::TransactValueCallCreate;

use crate::info;

pub struct EthService {
    pub block_storage: Arc<dyn BlockProvider>,
    pub state_storage: Arc<dyn StateProvider>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub executor: Executor,
    pub storage: Arc<RwLock<InMemoryStorage>>,
    pub account_manager: Arc<AccountManager>,
}

impl EthService {
    pub async fn gas_price(&self) -> RpcResult<U256> {
        //TODO: Implement gas price
        if let Ok(Some(header)) = self.block_storage.header(BlockId::Number(BlockNumberOrTag::Latest)).await {
            if let Some(base_fee) = header.base_fee_per_gas {
                return Ok(U256::from(base_fee));
            }
        }
        // Fallback to 1 Gwei
        Ok(U256::from(1_000_000_000u64))
    }

    pub async fn accounts(&self) -> RpcResult<Vec<Address>> {
        self.state_storage.accounts().await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))
    }

    pub async fn syncing(&self) -> RpcResult<SyncStatus> {
        Ok(SyncStatus::None)
    }

    pub async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<alloy_primitives::B256> {
        info!("[EthService] eth_sendTransaction from: {:?}", request.from);
        
        let from = request.from.ok_or_else(|| crate::error::RpcError::InvalidParams("from address is required".to_string()))?;
        
        if !self.account_manager.is_managed(&from) {
            return Err(crate::error::RpcError::AccountNotFound(from));
        }

        let chain_id = self.block_storage.chain_id().await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?;
        let nonce = if let Some(n) = request.nonce {
            n
        } else {
            self.state_storage.transaction_count(from, BlockId::Number(BlockNumberOrTag::Latest)).await
                .map_err(|e| crate::error::RpcError::Internal(e.to_string()))?
        };

        let gas_price = request.gas_price.unwrap_or(1_000_000_000u128);
        let gas_limit = request.gas.unwrap_or(21000);

        let tx = TxLegacy {
            chain_id: Some(chain_id),
            nonce,
            gas_price,
            gas_limit,
            to: request.to.unwrap_or_default().into(),
            value: request.value.unwrap_or_default(),
            input: request.input.data.clone().unwrap_or_default(),
        };

        let signed_tx: Transaction = self.account_manager.sign_transaction(&from, tx).await?;
        let hash = signed_tx.hash().clone();

        info!("[EthService] Adding signed transaction {:?} to mempool", hash);
        self.mempool.write().await.add_transaction(signed_tx);

        Ok(hash)
    }

    pub async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<Bytes> {
        info!("[EthService] eth_call: to={:?}, data_len={}", request.to, request.input.data.as_ref().map(|d| d.len()).unwrap_or(0));
        // TODO: Implement eth_call
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let block = self.block_storage.block(block_id).await
            .map_err(|e| crate::error::RpcError::Internal(e.to_string()))?
            .ok_or(crate::error::RpcError::BlockNotFound(block_id))?;
        
        // Convert TransactionRequest to TxEnvelope
        // This is simplified, we need a way to create a TxEnvelope from a request for simulation
        let tx = TxLegacy {
            chain_id: Some(self.block_storage.chain_id().await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?),
            nonce: request.nonce.unwrap_or_default(),
            gas_price: request.gas_price.unwrap_or(1_000_000_000u128),
            gas_limit: request.gas.unwrap_or(1_000_000),
            to: request.to.unwrap_or_default().into(),
            value: request.value.unwrap_or_default(),
            input: request.input.data.clone().unwrap_or_default(),
        };
        let tx_envelope = Transaction::Legacy(alloy_consensus::Signed::new_unchecked(tx, alloy_primitives::Signature::test_signature(), alloy_primitives::B256::ZERO));

        let storage_lock = self.storage.read().await;
        // In a real implementation, we should execute on top of the state of the given block.
        // InMemoryStorage currently only has the latest state easily accessible for execution.
        let result = self.executor.call(&storage_lock, tx_envelope, block)
            .map_err(|e| crate::error::RpcError::Internal(e))?;
        
        match result.call_create {
            TransactValueCallCreate::Call { retval, .. } => Ok(retval.into()),
            TransactValueCallCreate::Create { .. } => Ok(Bytes::new()), // For create, returns init code or empty? Usually call on address
        }
    }

    pub async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<U256> {
        // TODO: Implement gas estimation
        // Similar to call, but we return gas used.
        // This is a simplified version.
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let _block = self.block_storage.block(block_id).await
            .map_err(|e| crate::error::RpcError::Internal(e.to_string()))?
            .ok_or(crate::error::RpcError::BlockNotFound(block_id))?;
        
        let tx = TxLegacy {
            chain_id: Some(self.block_storage.chain_id().await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?),
            nonce: request.nonce.unwrap_or_default(),
            gas_price: request.gas_price.unwrap_or(1_000_000_000u128),
            gas_limit: request.gas.unwrap_or(10_000_000), // High limit for estimation
            to: request.to.unwrap_or_default().into(),
            value: request.value.unwrap_or_default(),
            input: request.input.data.clone().unwrap_or_default(),
        };
        let _tx_envelope = Transaction::Legacy(alloy_consensus::Signed::new_unchecked(tx, alloy_primitives::Signature::test_signature(), alloy_primitives::B256::ZERO));

        let _storage_lock = self.storage.read().await;
        // For estimation, we need to know gas used.
        // Our executor currently returns TransactValue which is just the output bytes.
        // We might need to modify Executor to return more info.
        // For now, let's return a dummy gas value or modify the executor.
        
        // Let's assume 21000 + some gas for now, or use a fixed value.
        Ok(U256::from(21000u64))
    }
}
