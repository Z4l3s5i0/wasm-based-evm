use crate::evm::executor::Executor;
use crate::mempool::Mempool;
use crate::misc::error::{RpcError, RpcResult};
use crate::rpc::engine_mapper::EngineMapper;
use crate::storage::storage::InMemoryStorage;
use crate::sync::controller::SyncController;
use crate::{error, info, debug};
use alloy_consensus::{Block, Header, ReceiptWithBloom as Receipt, Transaction, TxEnvelope};
use alloy_primitives::{Bytes, B256, U256};
use alloy_rlp::Decodable;
use alloy_rpc_types::engine::{
    ExecutionPayloadBodyV1, ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3,
    ExecutionPayloadV4, ForkchoiceState, ForkchoiceUpdated, PayloadAttributes, PayloadId,
    PayloadStatus, PayloadStatusEnum, TransitionConfiguration, ExecutionPayloadEnvelopeV2,
};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct EngineService {
    pub storage: Arc<RwLock<InMemoryStorage>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub executor: Executor,
    pub sync_engine: Arc<SyncController>,
}

impl EngineService {
    pub fn new(
        storage: Arc<RwLock<InMemoryStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        executor: Executor,
        sync_engine: Arc<SyncController>,
    ) -> Self {
        info!("[EngineService] Initializing engine service");
        Self {
            storage,
            mempool,
            executor,
            sync_engine,
        }
    }

    pub async fn exchange_capabilities(&self, _capabilities: Vec<String>) -> RpcResult<Vec<String>> {
        Ok(vec![
            "engine_exchangeCapabilities".to_string(),
            "engine_forkchoiceUpdatedV1".to_string(),
            "engine_forkchoiceUpdatedV2".to_string(),
            "engine_newPayloadV1".to_string(),
            "engine_newPayloadV2".to_string(),
            "engine_getPayloadV1".to_string(),
            "engine_getPayloadV2".to_string(),
            "engine_exchangeTransitionConfigurationV1".to_string(),
            "engine_getPayloadBodiesByHashV1".to_string(),
            "engine_getPayloadBodiesByHashV2".to_string(),
            "engine_getPayloadBodiesByRangeV1".to_string(),
            "engine_getPayloadBodiesByRangeV2".to_string(),
        ])
    }

    pub async fn forkchoice_updated_v3(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        self.forkchoice_updated(forkchoice_state, payload_attributes, 3).await
    }

    pub async fn forkchoice_updated_v4(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        self.forkchoice_updated(forkchoice_state, payload_attributes, 4).await
    }

    pub async fn exchange_transition_configuration_v1(
        &self,
        config: TransitionConfiguration,
    ) -> RpcResult<TransitionConfiguration> {
        Ok(config)
    }

    pub async fn get_blobs_v1(&self, _indices: Vec<B256>) -> RpcResult<Vec<Option<String>>> {
        // TODO Blobs are not yet supported in this execution engine.
        // Returning a vector of None for the requested indices to indicate unavailability.
        Ok(vec![None; _indices.len()])
    }

    pub async fn get_blobs_v2(&self, _indices: Vec<B256>) -> RpcResult<Vec<Option<String>>> {
        self.get_blobs_v1(_indices).await
    }

    pub async fn get_blobs_v3(&self, _indices: Vec<B256>) -> RpcResult<Vec<Option<String>>> {
        self.get_blobs_v1(_indices).await
    }

    pub async fn get_payload_bodies_by_hash_v1(
        &self,
        hashes: Vec<B256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        let storage = self.storage.read().await;
        let mut bodies = Vec::new();
        for hash in hashes {
            let body = storage.get_block_by_hash(hash).map(|block| {
                EngineMapper::to_execution_payload_body_v1(&block)
            });
            bodies.push(body);
        }
        Ok(bodies)
    }

    pub async fn get_payload_bodies_by_hash_v2(
        &self,
        hashes: Vec<B256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        self.get_payload_bodies_by_hash_v1(hashes).await
    }

    pub async fn get_payload_bodies_by_range_v1(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        let storage = self.storage.read().await;
        let mut bodies = Vec::new();
        for i in 0..count {
            let number = start + i;
            let body = storage.get_block_by_number(number).map(|block| {
                EngineMapper::to_execution_payload_body_v1(&block)
            });
            bodies.push(body);
        }
        Ok(bodies)
    }

    pub async fn get_payload_bodies_by_range_v2(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        self.get_payload_bodies_by_range_v1(start, count).await
    }

    pub async fn get_payload_v3(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV3> {
        let storage = self.storage.read().await;
        let (block, _) = storage.get_payload(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        Ok(EngineMapper::to_execution_payload_v3(&block.clone()))
    }

    pub async fn get_payload_v4(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4> {
        let storage = self.storage.read().await;
        let (block, _) = storage.get_payload(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        Ok(EngineMapper::to_execution_payload_v4(&block.clone()))
    }

    pub async fn get_payload_v5(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4> {
        // V5/V6 currently use V4 structure in alloy-rpc-types but may have different semantics
        // or additional context-dependent fields. For now, we return the V4 representation.
        self.get_payload_v4(payload_id).await
    }

    pub async fn get_payload_v6(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4> {
        self.get_payload_v4(payload_id).await
    }

    pub async fn new_payload_v3(&self, payload: ExecutionPayloadV3) -> RpcResult<PayloadStatus> {
        // V3 introduces withdrawals and blobs. We pass withdrawals to the internal executor.
        self.new_payload(payload.payload_inner.payload_inner, Some(payload.payload_inner.withdrawals)).await
    }

    pub async fn new_payload_v4(&self, payload: ExecutionPayloadV4) -> RpcResult<PayloadStatus> {
        // V4 adds consolidation requests and other Cancun/Deneb features.
        // We reuse the V3 execution logic which handles transactions and withdrawals.
        self.new_payload(
            payload.payload_inner.payload_inner.payload_inner,
            Some(payload.payload_inner.payload_inner.withdrawals)
        ).await
    }

    pub async fn new_payload_v5(&self, payload: ExecutionPayloadV4) -> RpcResult<PayloadStatus> {
        // V5 continues with V4 structure for now.
        self.new_payload_v4(payload).await
    }

    pub async fn forkchoice_updated_v1(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        self.forkchoice_updated(forkchoice_state, payload_attributes, 1).await
    }

    pub async fn forkchoice_updated_v2(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        self.forkchoice_updated(forkchoice_state, payload_attributes, 2).await
    }

    async fn forkchoice_updated(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
        _version: u8,
    ) -> RpcResult<ForkchoiceUpdated> {
        debug!("[EngineService] forkchoiceUpdated: head={:?}, payload_attributes={:?}", forkchoice_state.head_block_hash, payload_attributes);
        let mut storage = self.storage.write().await;

        // Save current forkchoice state for potential rollback
        let old_head = storage.head_block_hash;
        let old_safe = storage.safe_block_hash;
        let old_finalized = storage.finalized_block_hash;

        // Update forkchoice in storage
        storage.update_forkchoice(forkchoice_state.head_block_hash, Some(forkchoice_state.safe_block_hash), Some(forkchoice_state.finalized_block_hash));

        // 1. Determine payload status based on head block availability
        let status = self.determine_payload_status(&storage, forkchoice_state.head_block_hash).await;

        // 2. Build payload if requested and status is VALID (or SYNCING)
        let (status, payload_id) = if status.status == PayloadStatusEnum::Valid || (status.status == PayloadStatusEnum::Syncing && payload_attributes.is_some()) {
            if let Some(attr) = payload_attributes {
                match self.build_new_payload(&mut storage, forkchoice_state.head_block_hash, attr, &status).await {
                    Ok(id) => (status, id),
                    Err(RpcError::InvalidParams(e)) if e == "Invalid timestamp" => {
                        (PayloadStatus {
                            status: PayloadStatusEnum::Invalid { validation_error: e },
                            latest_valid_hash: Some(forkchoice_state.head_block_hash),
                        }, None)
                    }
                    Err(e) => {
                        // Rollback forkchoice in storage on error
                        storage.update_forkchoice(old_head, Some(old_safe), Some(old_finalized));
                        return Err(e);
                    }
                }
            } else {
                (status, None)
            }
        } else {
            (status, None)
        };

        Ok(ForkchoiceUpdated {
            payload_status: status,
            payload_id,
        })
    }

    async fn determine_payload_status(&self, storage: &InMemoryStorage, head_block_hash: B256) -> PayloadStatus {
        if let Some(head_block) = storage.get_block_by_hash(head_block_hash) {
            debug!("[EngineService] Head block found in storage: #{} hash={:?}", head_block.header.number, head_block_hash);
            PayloadStatus {
                status: PayloadStatusEnum::Valid,
                latest_valid_hash: Some(head_block_hash),
            }
        } else if let Some((payload_block, _)) = storage.payloads.values().find(|(b, _)| b.header.hash_slow() == head_block_hash) {
            debug!("[EngineService] Head block found in payload map: #{} hash={:?}", payload_block.header.number, head_block_hash);
            PayloadStatus {
                status: PayloadStatusEnum::Valid,
                latest_valid_hash: Some(head_block_hash),
            }
        } else {
            // Check if requested head is actually the genesis block (by hash)
            let genesis_hash = storage.get_block_by_number(0).map(|b| b.header.hash_slow());
            if genesis_hash == Some(head_block_hash) {
                debug!("[EngineService] Head block is GENESIS: hash={:?}", head_block_hash);
                PayloadStatus {
                    status: PayloadStatusEnum::Valid,
                    latest_valid_hash: Some(head_block_hash),
                }
            } else {
                let local_head_hash = storage.head_block_hash;
                debug!("[EngineService] Head block NOT found: requested_hash={:?}. Local head is {:?} (genesis is {:?}). Returning Syncing.", 
                    head_block_hash, local_head_hash, genesis_hash);

                // Trigger sync for missing head
                let sync_engine = self.sync_engine.clone();
                tokio::spawn(async move {
                    if let Err(e) = sync_engine.trigger_sync().await {
                        error!("[EngineService] Failed to trigger sync for missing head: {}", e);
                    }
                });

                PayloadStatus {
                    status: PayloadStatusEnum::Syncing,
                    latest_valid_hash: None,
                }
            }
        }
    }

    async fn build_new_payload(
        &self,
        storage: &mut InMemoryStorage,
        head_block_hash: B256,
        attr: PayloadAttributes,
        status: &PayloadStatus,
    ) -> RpcResult<Option<PayloadId>> {
        if status.status == PayloadStatusEnum::Syncing {
            debug!("[EngineService] Cannot build payload: head block missing (status is Syncing)");
            return Ok(None);
        }

        // Validate attributes
        let parent_block = storage.get_block_by_hash(head_block_hash)
            .cloned()
            .or_else(|| {
                // Fallback to payloads map if head is not in storage yet
                storage.payloads.values()
                    .find(|(b, _)| b.header.hash_slow() == head_block_hash)
                    .map(|(b, _)| b.clone())
            })
            .ok_or_else(|| RpcError::InvalidForkchoiceState(format!("Head block not found for payload building: {:?}", head_block_hash)))?;

        if attr.timestamp <= parent_block.header.timestamp {
            return Err(RpcError::InvalidParams("Invalid timestamp".to_string()));
        }

        let id = self.generate_payload_id(&head_block_hash, &attr);

        // Calculate base fee (simplified EIP-1559)
        let base_fee_per_gas = self.calculate_next_base_fee(&parent_block.header);

        // Build block logic
        let transactions = {
            let mempool = self.mempool.read().await;
            //TODO calculate how many actually fit in the block
            mempool.peek_transactions(50) // Take top 50 transactions
        };

        let (finalized_block, receipts) = self.executor.execute_block_for_payload(
            storage,
            transactions,
            &parent_block.header,
            &attr,
            base_fee_per_gas,
        ).map_err(|e| RpcError::Internal(format!("Failed to build block: {}", e)))?;

        storage.add_payload(id, finalized_block, receipts);
        debug!("[EngineService] Created payload_id={:?} for block_number={}", id, parent_block.header.number + 1);
        
        Ok(Some(id))
    }

    fn generate_payload_id(&self, head_block_hash: &B256, attr: &PayloadAttributes) -> PayloadId {
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

    fn calculate_next_base_fee(&self, parent_header: &Header) -> Option<u64> {
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
            Some(1_000_000_000) // Default to 1 Gwei
        }
    }

    pub async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1> {
        let storage = self.storage.read().await;
        let (block, _) = storage.get_payload(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        Ok(EngineMapper::to_execution_payload_v1(block))
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

    pub async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadEnvelopeV2> {
        let storage = self.storage.read().await;
        let (block, receipts) = storage.get_payload(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        let execution_payload = EngineMapper::to_execution_payload_v2(block);
        let block_value = self.calculate_block_value(block, receipts);
        
        Ok(EngineMapper::to_execution_payload_envelope_v2(execution_payload, block_value))
    }

    pub async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> RpcResult<PayloadStatus> {
        self.new_payload(payload.into(), None).await
    }

    pub async fn new_payload_v2(&self, payload: ExecutionPayloadV2) -> RpcResult<PayloadStatus> {
        self.new_payload(payload.payload_inner.into(), Some(payload.withdrawals)).await
    }

    async fn new_payload(
        &self,
        payload_v1: ExecutionPayloadV1,
        withdrawals: Option<Vec<alloy_rpc_types::Withdrawal>>,
    ) -> RpcResult<PayloadStatus> {
        debug!("[EngineService] newPayload: block_number={}, block_hash={:?}, parent_hash={:?}", payload_v1.block_number, payload_v1.block_hash, payload_v1.parent_hash);
        
        // 1. Convert ExecutionPayload to Block
        let transactions = self.decode_transactions(&payload_v1.transactions)?;
        let block = EngineMapper::payload_v1_to_block(&payload_v1, transactions.clone(), withdrawals);

        // Ensure block hash matches
        let actual_hash = block.header.hash_slow();
        if actual_hash != payload_v1.block_hash {
             return Ok(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: "Block hash mismatch".to_string() },
                latest_valid_hash: None,
            });
        }

        // 2. Validate parent and Execute Block
        let mut storage = self.storage.write().await;
        if let Some(status) = self.validate_parent_block(&storage, payload_v1.parent_hash) {
            return Ok(status);
        }

        match self.executor.execute_block(&mut storage, transactions, block) {
            Ok(_) => {
                Ok(PayloadStatus {
                    status: PayloadStatusEnum::Valid,
                    latest_valid_hash: Some(actual_hash),
                })
            }
            Err(e) => {
                Ok(PayloadStatus {
                    status: PayloadStatusEnum::Invalid { validation_error: e },
                    latest_valid_hash: None,
                })
            }
        }
    }

    fn decode_transactions(&self, txs: &[Bytes]) -> RpcResult<Vec<TxEnvelope>> {
        let mut transactions = Vec::new();
        for tx_bytes in txs {
            let tx: TxEnvelope = Decodable::decode(&mut &tx_bytes[..])
                .map_err(|e| RpcError::InvalidParams(format!("Failed to decode transaction: {}", e)))?;
            transactions.push(tx);
        }
        Ok(transactions)
    }


    fn validate_parent_block(&self, storage: &InMemoryStorage, parent_hash: B256) -> Option<PayloadStatus> {
        if storage.get_block_by_hash(parent_hash).is_none() {
            let genesis_hash = storage.get_block_by_number(0).map(|b| b.header.hash_slow());
            if Some(parent_hash) != genesis_hash {
                debug!("[EngineService] Parent block not found: requested_parent={:?}, genesis={:?}", parent_hash, genesis_hash);
                return Some(PayloadStatus {
                    status: PayloadStatusEnum::Syncing,
                    latest_valid_hash: None,
                });
            }
        }
        None
    }
}
