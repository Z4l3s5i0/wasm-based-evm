use std::sync::Arc;
use crate::error::RpcResult;
use alloy_primitives::{Address, U256};
use alloy_eips::BlockId;
use crate::storage::traits::StateProvider;

pub struct AccountService {
    pub storage: Arc<dyn StateProvider>,
}

impl AccountService {
    pub async fn get_balance(&self, address: Address, block_id: BlockId) -> RpcResult<U256> {
        self.storage.balance(address, block_id).map_err(|e| crate::error::RpcError::Internal(e.to_string()))
    }

    pub async fn get_transaction_count(&self, address: Address, block_id: BlockId) -> RpcResult<u64> {
        self.storage.transaction_count(address, block_id).map_err(|e| crate::error::RpcError::Internal(e.to_string()))
    }
}