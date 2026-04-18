use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use crate::error::{RpcResult, RpcError};
use alloy_rpc_types::engine::{
    ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3, ExecutionPayloadV4, 
    ForkchoiceState, ForkchoiceUpdated, PayloadAttributes, PayloadId, PayloadStatus, 
    PayloadStatusEnum, TransitionConfiguration, ExecutionPayloadBodyV1,
};
use alloy_rpc_types::SyncStatus;
use alloy_consensus::{Block, Header, TxEnvelope as Transaction};
use alloy_primitives::{B256, U256, Bytes};
use crate::{info, error};
use crate::storage::storage::InMemoryStorage;
use crate::mempool::Mempool;
use crate::executor::Executor;
use crate::p2p::sync::SyncEngine;
use alloy_rlp::Decodable;

pub struct EngineService {
    pub storage: Arc<RwLock<InMemoryStorage>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub executor: Executor,
    pub sync_engine: Arc<SyncEngine>,
    pub payloads: Arc<RwLock<HashMap<PayloadId, Block<Transaction>>>>,
}

impl EngineService {
    pub fn new(
        storage: Arc<RwLock<InMemoryStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        executor: Executor,
        sync_engine: Arc<SyncEngine>,
    ) -> Self {
        info!("[EngineService] Initializing engine service");
        Self {
            storage,
            mempool,
            executor,
            sync_engine,
            payloads: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn exchange_capabilities(&self, _capabilities: Vec<String>) -> RpcResult<Vec<String>> {
        Ok(vec![
            "engine_exchangeCapabilities".to_string(),
            // "engine_exchangeTransitionConfigurationV1".to_string(),
            "engine_forkchoiceUpdatedV1".to_string(),
            "engine_forkchoiceUpdatedV2".to_string(),
            // "engine_forkchoiceUpdatedV3".to_string(),
            // "engine_forkchoiceUpdatedV4".to_string(),
            // "engine_getBlobsV1".to_string(),
            // "engine_getBlobsV2".to_string(),
            // "engine_getBlobsV3".to_string(),
            "engine_getPayloadBodiesByHashV1".to_string(),
            "engine_getPayloadBodiesByHashV2".to_string(),
            "engine_getPayloadBodiesByRangeV1".to_string(),
            "engine_getPayloadBodiesByRangeV2".to_string(),
            "engine_getPayloadV1".to_string(),
            "engine_getPayloadV2".to_string(),
            // "engine_getPayloadV3".to_string(),
            // "engine_getPayloadV4".to_string(),
            // "engine_getPayloadV5".to_string(),
            // "engine_getPayloadV6".to_string(),
            "engine_newPayloadV1".to_string(),
            "engine_newPayloadV2".to_string(),
            // "engine_newPayloadV3".to_string(),
            // "engine_newPayloadV4".to_string(),
            // "engine_newPayloadV5".to_string(),
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
        // Blobs are not yet supported in this execution engine.
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
                ExecutionPayloadBodyV1 {
                    transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
                    withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()),
                }
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
                ExecutionPayloadBodyV1 {
                    transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
                    withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()),
                }
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
        let payloads_lock = self.payloads.read().await;
        let block = payloads_lock.get(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        Ok(ExecutionPayloadV3 {
            payload_inner: ExecutionPayloadV2 {
                payload_inner: ExecutionPayloadV1 {
                    parent_hash: block.header.parent_hash,
                    fee_recipient: block.header.beneficiary,
                    state_root: block.header.state_root,
                    receipts_root: block.header.receipts_root,
                    logs_bloom: block.header.logs_bloom,
                    prev_randao: block.header.mix_hash,
                    block_number: block.header.number,
                    gas_limit: block.header.gas_limit as u64,
                    gas_used: block.header.gas_used as u64,
                    timestamp: block.header.timestamp,
                    extra_data: block.header.extra_data.clone(),
                    base_fee_per_gas: U256::from(block.header.base_fee_per_gas.unwrap_or_default()),
                    block_hash: block.header.hash_slow(),
                    transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
                },
                withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()).unwrap_or_default(),
            },
            blob_gas_used: 0,
            excess_blob_gas: 0,
        })
    }

    pub async fn get_payload_v4(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4> {
        let payloads_lock = self.payloads.read().await;
        let block = payloads_lock.get(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        Ok(ExecutionPayloadV4 {
            payload_inner: ExecutionPayloadV3 {
                payload_inner: ExecutionPayloadV2 {
                    payload_inner: ExecutionPayloadV1 {
                        parent_hash: block.header.parent_hash,
                        fee_recipient: block.header.beneficiary,
                        state_root: block.header.state_root,
                        receipts_root: block.header.receipts_root,
                        logs_bloom: block.header.logs_bloom,
                        prev_randao: block.header.mix_hash,
                        block_number: block.header.number,
                        gas_limit: block.header.gas_limit as u64,
                        gas_used: block.header.gas_used as u64,
                        timestamp: block.header.timestamp,
                        extra_data: block.header.extra_data.clone(),
                        base_fee_per_gas: U256::from(block.header.base_fee_per_gas.unwrap_or_default()),
                        block_hash: block.header.hash_slow(),
                        transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
                    },
                    withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()).unwrap_or_default(),
                },
                blob_gas_used: 0,
                excess_blob_gas: 0,
            },
            block_access_list: vec![].into(),
        })
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
        info!("[EngineService] forkchoiceUpdated: head={:?}, payload_attributes={:?}", forkchoice_state.head_block_hash, payload_attributes);
        let mut storage = self.storage.write().await;

        // Update forkchoice in storage
        storage.update_forkchoice(forkchoice_state.head_block_hash, Some(forkchoice_state.safe_block_hash), Some(forkchoice_state.finalized_block_hash));

        // Check if head block is in storage
        let status = if storage.get_block_by_hash(forkchoice_state.head_block_hash).is_none() {
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
        } else {
            PayloadStatus {
                status: PayloadStatusEnum::Valid,
                latest_valid_hash: Some(forkchoice_state.head_block_hash),
            }
        };

        let mut payload_id = None;
        if status.status == PayloadStatusEnum::Valid {
            if let Some(attr) = payload_attributes {
                // Validate attributes
                let parent_block = storage.get_block_by_hash(forkchoice_state.head_block_hash)
                    .cloned()
                    .ok_or_else(|| RpcError::InvalidForkchoiceState(format!("Head block not found: {:?}", forkchoice_state.head_block_hash)))?;

                if attr.timestamp <= parent_block.header.timestamp {
                    return Ok(ForkchoiceUpdated {
                        payload_status: PayloadStatus {
                            status: PayloadStatusEnum::Invalid { validation_error: "Invalid timestamp".to_string() },
                            latest_valid_hash: Some(forkchoice_state.head_block_hash),
                        },
                        payload_id: None,
                    });
                }

                // Start building a block
                let id = PayloadId::new(rand::random());
                payload_id = Some(id);

                // Build block logic (simplified)
                let mempool = self.mempool.read().await;
                let transactions = mempool.peek_transactions(10); // Take top 10 transactions

            let header = Header {
                parent_hash: forkchoice_state.head_block_hash,
                number: parent_block.header.number + 1,
                timestamp: attr.timestamp,
                beneficiary: attr.suggested_fee_recipient,
                gas_limit: parent_block.header.gas_limit,
                base_fee_per_gas: parent_block.header.base_fee_per_gas,
                extra_data: Bytes::new(),
                mix_hash: attr.prev_randao,
                ..Default::default()
            };

            let block = Block {
                header,
                body: alloy_consensus::BlockBody {
                    transactions,
                    ommers: vec![],
                    withdrawals: attr.withdrawals.clone().map(|w| alloy_eips::eip4895::Withdrawals::new(w.into_iter().map(|wi| alloy_eips::eip4895::Withdrawal {
                        index: wi.index,
                        validator_index: wi.validator_index,
                        address: wi.address,
                        amount: wi.amount,
                    }).collect())),
                },
            };

            self.payloads.write().await.insert(id, block);
            info!("[EngineService] Created payload_id={:?} for block_number={}", id, parent_block.header.number + 1);
            }
        }

        Ok(ForkchoiceUpdated {
            payload_status: status,
            payload_id,
        })
    }

    pub async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1> {
        let payloads_lock = self.payloads.read().await;
        let block = payloads_lock.get(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        // Convert Block to ExecutionPayloadV1
        // This requires mapping all fields precisely
        Ok(ExecutionPayloadV1 {
            parent_hash: block.header.parent_hash,
            fee_recipient: block.header.beneficiary,
            state_root: block.header.state_root,
            receipts_root: block.header.receipts_root,
            logs_bloom: block.header.logs_bloom,
            prev_randao: block.header.mix_hash,
            block_number: block.header.number,
            gas_limit: block.header.gas_limit as u64,
            gas_used: block.header.gas_used as u64,
            timestamp: block.header.timestamp,
            extra_data: block.header.extra_data.clone(),
            base_fee_per_gas: U256::from(block.header.base_fee_per_gas.unwrap_or_default()),
            block_hash: block.header.hash_slow(),
            transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
        })
    }

    pub async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV2> {
        let payloads_lock = self.payloads.read().await;
        let block = payloads_lock.get(&payload_id)
            .ok_or_else(|| RpcError::UnknownPayload(format!("Payload not found: {:?}", payload_id)))?;

        Ok(ExecutionPayloadV2 {
            payload_inner: ExecutionPayloadV1 {
                parent_hash: block.header.parent_hash,
                fee_recipient: block.header.beneficiary,
                state_root: block.header.state_root,
                receipts_root: block.header.receipts_root,
                logs_bloom: block.header.logs_bloom,
                prev_randao: block.header.mix_hash,
                block_number: block.header.number,
                gas_limit: block.header.gas_limit as u64,
                gas_used: block.header.gas_used as u64,
                timestamp: block.header.timestamp,
                extra_data: block.header.extra_data.clone(),
                base_fee_per_gas: U256::from(block.header.base_fee_per_gas.unwrap_or_default()),
                block_hash: block.header.hash_slow(),
                transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
            },
            withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()).unwrap_or_default(),
        })
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
        info!("[EngineService] newPayload: block_number={}, block_hash={:?}, parent_hash={:?}", payload_v1.block_number, payload_v1.block_hash, payload_v1.parent_hash);
        // 1. Convert ExecutionPayload to Block
        let mut transactions = Vec::new();
        for tx_bytes in &payload_v1.transactions {
            let tx: Transaction = Decodable::decode(&mut &tx_bytes[..])
                .map_err(|e| RpcError::InvalidParams(format!("Failed to decode transaction: {}", e)))?;
            transactions.push(tx);
        }

        let header = Header {
            parent_hash: payload_v1.parent_hash,
            beneficiary: payload_v1.fee_recipient,
            state_root: payload_v1.state_root,
            receipts_root: payload_v1.receipts_root,
            logs_bloom: payload_v1.logs_bloom,
            mix_hash: payload_v1.prev_randao,
            number: payload_v1.block_number,
            gas_limit: payload_v1.gas_limit,
            gas_used: payload_v1.gas_used,
            timestamp: payload_v1.timestamp,
            extra_data: payload_v1.extra_data.clone(),
            base_fee_per_gas: Some(payload_v1.base_fee_per_gas.to::<u128>().try_into().unwrap()),
            ..Default::default()
        };

        // Ensure block hash matches
        let actual_hash = header.hash_slow();
        if actual_hash != payload_v1.block_hash {
             return Ok(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: "Block hash mismatch".to_string() },
                latest_valid_hash: None,
            });
        }

        let block = Block {
            header,
            body: alloy_consensus::BlockBody {
                transactions: transactions.clone(),
                ommers: vec![],
                withdrawals: withdrawals.map(|w: Vec<alloy_rpc_types::Withdrawal>| alloy_eips::eip4895::Withdrawals::new(w.into_iter().map(|wi| alloy_eips::eip4895::Withdrawal {
                    index: wi.index,
                    validator_index: wi.validator_index,
                    address: wi.address,
                    amount: wi.amount,
                }).collect())),
            },
        };

        // 2. Execute Block
        let mut storage = self.storage.write().await;

        // Check for parent block - if missing, return SYNCING
        if storage.get_block_by_hash(payload_v1.parent_hash).is_none() {
            return Ok(PayloadStatus {
                status: PayloadStatusEnum::Syncing,
                latest_valid_hash: None,
            });
        }

        match self.executor.execute_block(&mut storage, transactions, block.clone()) {
            Ok(_) => {
                // 3. Add to storage
                storage.add_block(block);
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
}
