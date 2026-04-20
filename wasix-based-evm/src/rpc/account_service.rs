use std::sync::Arc;
use alloy_primitives::{Address, U256, B256, Bytes};
use alloy_eips::BlockId;
use crate::misc::error::RpcError::Internal;
use crate::misc::error::RpcResult;
use crate::storage::traits::StateProvider;

pub struct AccountService {
    pub storage: Arc<dyn StateProvider>,
}

impl AccountService {
    pub async fn get_balance(&self, address: Address, block_id: BlockId) -> RpcResult<U256> {
        self.storage.balance(address, block_id).await.map_err(|e| Internal(e.to_string()))
    }

    pub async fn get_transaction_count(&self, address: Address, block_id: BlockId) -> RpcResult<u64> {
        self.storage.transaction_count(address, block_id).await.map_err(|e| Internal(e.to_string()))
    }

    pub async fn get_code(&self, address: Address, block_id: BlockId) -> RpcResult<Bytes> {
        let code = self.storage.code(address, block_id).await
            .map_err(|e| Internal(e.to_string()))?;
        Ok(code.unwrap_or_default())
    }

    pub async fn get_storage_at(&self, address: Address, slot: B256, block_id: BlockId) -> RpcResult<U256> {
        let storage = self.storage.storage(address, slot, block_id).await
            .map_err(|e| Internal(e.to_string()))?;
        Ok(storage.unwrap_or_default())
    }
}