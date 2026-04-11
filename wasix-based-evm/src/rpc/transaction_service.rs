use crate::error::RpcResult;
use std::sync::Arc;
use alloy_primitives::B256;
use alloy_rpc_types::{Transaction, TransactionReceipt};
use crate::storage::traits::TransactionProvider;
use crate::rpc::transaction_mapper::TransactionMapper;

pub struct TransactionService {
    pub storage: Arc<dyn TransactionProvider>,
}

impl TransactionService {
    pub async fn get_transaction_by_hash(&self, hash: B256) -> RpcResult<Option<Transaction>> {
        let tx = self.storage.transaction(hash).await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?;
        let block_ref = self.storage.transaction_block_reference(hash).await.ok().flatten();

        Ok(tx.map(|t| TransactionMapper::to_rpc_transaction(t, block_ref)))
    }

    pub async fn get_transaction_receipt(&self, hash: B256) -> RpcResult<Option<TransactionReceipt>> {
        let receipt = self.storage.transaction_receipt(hash).await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?;
        let block_ref = self.storage.transaction_block_reference(hash).await.ok().flatten();

        Ok(receipt.map(|r| TransactionMapper::to_rpc_receipt(r, block_ref)))
    }
}
