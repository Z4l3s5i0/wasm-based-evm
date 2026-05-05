use crate::evm::executor::Executor;
use crate::misc::error::{RpcError, RpcResult};
use crate::storage::mempool::Mempool;
use crate::storage::storage::InMemoryStorage;
use crate::storage::traits::SyncStateProvider;
use crate::debug;
use alloy_consensus::{Block, ReceiptWithBloom as Receipt, Header, TxEnvelope, Transaction as _};
use alloy_primitives::{B256, U256};
use alloy_rpc_types::engine::{PayloadAttributes, PayloadId, PayloadStatus, PayloadStatusEnum};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct PayloadBuilder {
    pub mempool: Arc<RwLock<Mempool>>,
    pub executor: Executor,
}

impl PayloadBuilder {
    pub fn new(mempool: Arc<RwLock<Mempool>>, executor: Executor) -> Self {
        Self { mempool, executor }
    }

    pub async fn build_new_payload(
        &self,
        storage: &mut InMemoryStorage,
        head_block_hash: B256,
        attr: PayloadAttributes,
        status: &PayloadStatus,
    ) -> RpcResult<Option<PayloadId>> {
        if status.status == PayloadStatusEnum::Syncing {
            debug!("[PayloadBuilder] Cannot build payload: head block missing (status is Syncing)");
            return Ok(None);
        }

        let parent_block = storage.get_block_by_hash(head_block_hash)
            .cloned()
            .or_else(|| {
                storage.chain.payloads.values()
                    .find(|(b, _)| b.header.hash_slow() == head_block_hash)
                    .map(|(b, _)| b.clone())
            })
            .ok_or_else(|| RpcError::InvalidForkchoiceState(format!("Head block not found for payload building: {:?}", head_block_hash)))?;

        if attr.timestamp <= parent_block.header.timestamp {
            return Err(RpcError::InvalidParams("Invalid timestamp".to_string()));
        }

        let id = self.generate_payload_id(&head_block_hash, &attr);
        let base_fee_per_gas = self.calculate_next_base_fee(&parent_block.header);

        let transactions = {
            let mempool = self.mempool.read().await;
            mempool.peek_transactions(50) 
        };

        let (finalized_block, receipts) = self.executor.execute_block_for_payload(
            storage as &mut dyn SyncStateProvider,
            transactions,
            &parent_block.header,
            &attr,
            base_fee_per_gas,
        ).map_err(|e| RpcError::Internal(format!("Failed to build block: {}", e)))?;

        storage.add_payload(id, finalized_block, receipts);
        debug!("[PayloadBuilder] Created payload_id={:?} for block_number={}", id, parent_block.header.number + 1);
        
        Ok(Some(id))
    }

    pub fn generate_payload_id(&self, head_block_hash: &B256, attr: &PayloadAttributes) -> PayloadId {
        use alloy_primitives::keccak256;
        let mut data = Vec::new();
        data.extend_from_slice(head_block_hash.as_slice());
        data.extend_from_slice(&attr.timestamp.to_be_bytes());
        data.extend_from_slice(attr.prev_randao.as_slice());
        data.extend_from_slice(attr.suggested_fee_recipient.as_slice());
        if let Some(withdrawals) = &attr.withdrawals {
            for w in withdrawals {
                data.extend_from_slice(&w.index.to_be_bytes());
                data.extend_from_slice(&w.validator_index.to_be_bytes());
                data.extend_from_slice(w.address.as_slice());
                data.extend_from_slice(&w.amount.to_be_bytes());
            }
        }
        if let Some(root) = attr.parent_beacon_block_root {
            data.extend_from_slice(root.as_slice());
        }

        let hash = keccak256(&data);
        PayloadId::new(hash[..8].try_into().unwrap())
    }

    pub fn calculate_next_base_fee(&self, parent_header: &Header) -> Option<u64> {
        if let Some(parent_fee) = parent_header.base_fee_per_gas {
            let parent_gas_used = parent_header.gas_used;
            let parent_gas_target = parent_header.gas_limit / 2;
            
            if parent_gas_used == parent_gas_target {
                Some(parent_fee)
            } else if parent_gas_used > parent_gas_target {
                let gas_used_delta = parent_gas_used - parent_gas_target;
                let fee_delta = std::cmp::max(1u64, (parent_fee as u128 * gas_used_delta as u128 / parent_gas_target as u128 / 8) as u64);
                Some(parent_fee + fee_delta)
            } else {
                let gas_used_delta = parent_gas_target - parent_gas_used;
                let fee_delta = (parent_fee as u128 * gas_used_delta as u128 / parent_gas_target as u128 / 8) as u64;
                Some(parent_fee.saturating_sub(fee_delta))
            }
        } else {
            Some(1_000_000_000) 
        }
    }

    pub fn calculate_block_value(&self, block: &Block<TxEnvelope>, receipts: &[Receipt]) -> U256 {
        let mut total_value = U256::ZERO;
        let base_fee = block.header.base_fee_per_gas.unwrap_or_default();

        let mut prev_cumulative_gas = 0u64;
        for (tx, receipt) in block.body.transactions.iter().zip(receipts.iter()) {
            let cumulative_gas = receipt.receipt.cumulative_gas_used;
            let gas_used = cumulative_gas.saturating_sub(prev_cumulative_gas);
            prev_cumulative_gas = cumulative_gas;
            
            let effective_gas_price = tx.max_fee_per_gas(); 
            let priority_fee = effective_gas_price.saturating_sub(base_fee as u128);
            total_value += U256::from(gas_used as u128 * priority_fee);
        }
        total_value
    }
}
