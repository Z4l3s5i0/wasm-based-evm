use crate::evm::executor::Executor;
use crate::misc::error::{RpcError, RpcResult};
use crate::rpc::engine_mapper::EngineMapper;
use crate::storage::storage::InMemoryStorage;
use crate::storage::traits::StateProvider;
use crate::sync::controller::SyncController;
use crate::{debug, error};
use alloy_consensus::TxEnvelope;
use alloy_primitives::{Bytes, B256};
use alloy_rlp::Decodable;
use alloy_rpc_types::engine::{ForkchoiceState, PayloadStatus, PayloadStatusEnum};
use std::sync::Arc;

pub struct BlockProcessor {
    pub state_storage: Arc<dyn StateProvider>,
    pub executor: Executor,
    pub sync_engine: Arc<SyncController>,
}

impl BlockProcessor {
    pub fn new(
        state_storage: Arc<dyn StateProvider>,
        executor: Executor,
        sync_engine: Arc<SyncController>,
    ) -> Self {
        Self {
            state_storage,
            executor,
            sync_engine,
        }
    }

    pub async fn process_new_payload(
        &self,
        payload_v1: alloy_rpc_types::engine::ExecutionPayloadV1,
        withdrawals: Option<Vec<alloy_rpc_types::Withdrawal>>,
    ) -> RpcResult<PayloadStatus> {
        debug!("[BlockProcessor] process_new_payload: block_number={}, block_hash={:?}, parent_hash={:?}", 
            payload_v1.block_number, payload_v1.block_hash, payload_v1.parent_hash);
        
        let transactions = self.decode_transactions(&payload_v1.transactions)?;
        let block = EngineMapper::payload_v1_to_block(&payload_v1, transactions.clone(), withdrawals);

        let actual_hash = block.header.hash_slow();
        if actual_hash != payload_v1.block_hash {
             return Ok(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: "Block hash mismatch".to_string() },
                latest_valid_hash: None,
            });
        }

        let mut storage_snapshot = self.state_storage.get_snapshot().await
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        if let Some(status) = self.validate_parent_block(storage_snapshot.as_any().downcast_ref::<InMemoryStorage>().unwrap(), payload_v1.parent_hash) {
            return Ok(status);
        }

        match self.executor.execute_block(storage_snapshot.as_any_mut().downcast_mut::<InMemoryStorage>().unwrap(), transactions.clone(), block.clone()) {
            Ok(result) => {
                let writer = self.state_storage.writer();
                writer.commit_block(result.finalized_block, result.receipts, result.changeset, result.withdrawals)
                    .await
                    .map_err(|e| RpcError::Internal(e.to_string()))?;

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

    pub async fn update_forkchoice(
        &self,
        forkchoice_state: ForkchoiceState,
    ) -> RpcResult<PayloadStatus> {
        debug!("[BlockProcessor] update_forkchoice: head={:?}", forkchoice_state.head_block_hash);
        
        let storage_snapshot = self.state_storage.get_snapshot().await
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        let storage = storage_snapshot.as_any().downcast_ref::<InMemoryStorage>().unwrap();

        // Save current forkchoice state for potential rollback is handled by the caller (EngineService) in this refactor step
        // But here we focus on the core logic of determining status and updating.
        
        let writer = self.state_storage.writer();
        writer.update_forkchoice(
            forkchoice_state.head_block_hash, 
            Some(forkchoice_state.safe_block_hash), 
            Some(forkchoice_state.finalized_block_hash)
        ).await.map_err(|e| RpcError::Internal(e.to_string()))?;

        Ok(self.determine_payload_status(storage, forkchoice_state.head_block_hash).await)
    }

    pub async fn determine_payload_status(&self, storage: &InMemoryStorage, head_block_hash: B256) -> PayloadStatus {
        if let Some(head_block) = storage.get_block_by_hash(head_block_hash) {
            debug!("[BlockProcessor] Head block found in storage: #{} hash={:?}", head_block.header.number, head_block_hash);
            PayloadStatus {
                status: PayloadStatusEnum::Valid,
                latest_valid_hash: Some(head_block_hash),
            }
        } else if let Some((payload_block, _)) = storage.chain.payloads.values().find(|(b, _)| b.header.hash_slow() == head_block_hash) {
            debug!("[BlockProcessor] Head block found in payload map: #{} hash={:?}", payload_block.header.number, head_block_hash);
            PayloadStatus {
                status: PayloadStatusEnum::Valid,
                latest_valid_hash: Some(head_block_hash),
            }
        } else {
            let genesis_hash = storage.get_block_by_number(0).map(|b| b.header.hash_slow());
            if genesis_hash == Some(head_block_hash) {
                debug!("[BlockProcessor] Head block is GENESIS: hash={:?}", head_block_hash);
                PayloadStatus {
                    status: PayloadStatusEnum::Valid,
                    latest_valid_hash: Some(head_block_hash),
                }
            } else {
                debug!("[BlockProcessor] Head block NOT found: requested_hash={:?}. Returning Syncing.", head_block_hash);

                let sync_engine = self.sync_engine.clone();
                tokio::spawn(async move {
                    if let Err(e) = sync_engine.trigger_sync().await {
                        error!("[BlockProcessor] Failed to trigger sync for missing head: {}", e);
                    }
                });

                PayloadStatus {
                    status: PayloadStatusEnum::Syncing,
                    latest_valid_hash: None,
                }
            }
        }
    }

    pub fn decode_transactions(&self, txs: &[Bytes]) -> RpcResult<Vec<TxEnvelope>> {
        let mut transactions = Vec::new();
        for tx_bytes in txs {
            let tx: TxEnvelope = Decodable::decode(&mut &tx_bytes[..])
                .map_err(|e| RpcError::InvalidParams(format!("Failed to decode transaction: {}", e)))?;
            transactions.push(tx);
        }
        Ok(transactions)
    }

    pub fn validate_parent_block(&self, storage: &InMemoryStorage, parent_hash: B256) -> Option<PayloadStatus> {
        if storage.get_block_by_hash(parent_hash).is_none() {
            let genesis_hash = storage.get_block_by_number(0).map(|b| b.header.hash_slow());
            if Some(parent_hash) != genesis_hash {
                debug!("[BlockProcessor] Parent block not found: requested_parent={:?}, genesis={:?}", parent_hash, genesis_hash);
                return Some(PayloadStatus {
                    status: PayloadStatusEnum::Syncing,
                    latest_valid_hash: None,
                });
            }
        }
        None
    }
}
