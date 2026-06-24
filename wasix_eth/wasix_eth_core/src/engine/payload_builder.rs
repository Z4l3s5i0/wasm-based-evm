use crate::mempool::mempool_provider::MempoolProvider;
use crate::Consensus;
use std::sync::Arc;
use wasix_eth_execution::execution_provider::ExecutionProvider;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::{BlockProvider, ChainProvider};
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::BlockWriter;
use wasix_eth_types::error::{RpcError, RpcResult};
use wasix_eth_types::Block;
use wasix_eth_types::ChainConfig;
use wasix_eth_types::PayloadAttributes;
use wasix_eth_types::PayloadId;
use wasix_eth_types::PayloadStatus;
use wasix_eth_types::PayloadStatusEnum;
use wasix_eth_types::Receipt;
use wasix_eth_types::Transaction;
use wasix_eth_types::B256;
use wasix_eth_types::U256;
use wasix_eth_types::BlobsBundleV1;
use wasix_eth_types::ConsensusTransaction;
use wasix_eth_utils::{debug, info, error, warn};
use wasix_eth_types::Result;

#[derive(Clone)]
pub struct PayloadBuilder {
    pub consensus: Arc<dyn Consensus>,
    pub read_storage: DatabaseReadProvider,
    pub write_storage: DatabaseWriteProvider,
    pub execution: Arc<dyn ExecutionProvider>,
    pub mempool: Arc<dyn MempoolProvider>,
}

impl PayloadBuilder {
    pub async fn generate_payload_id(&self, head_block_hash: &B256, attr: &PayloadAttributes) -> PayloadId {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        head_block_hash.hash(&mut hasher);
        attr.timestamp.hash(&mut hasher);
        attr.prev_randao.hash(&mut hasher);
        attr.suggested_fee_recipient.hash(&mut hasher);
        if let Some(withdrawals) = &attr.withdrawals {
            withdrawals.hash(&mut hasher);
        }
        if let Some(parent_beacon_block_root) = &attr.parent_beacon_block_root {
            parent_beacon_block_root.hash(&mut hasher);
        }
        let id_bytes = hasher.finish().to_be_bytes();
        PayloadId::new(id_bytes)
    }

    pub async fn build_new_payload(
        &self,
        head_block_hash: B256,
        attr: PayloadAttributes,
        status: &PayloadStatus,
    ) -> RpcResult<Option<PayloadId>> {
        if status.status == PayloadStatusEnum::Syncing {
            info!("[PayloadBuilder] Cannot build payload: head block missing (status is Syncing)");
            return Ok(None);
        }

        // Validate attributes
        let parent_block = self.read_storage.block_by_hash(head_block_hash).map_err(|e| RpcError::Internal(e.to_string()))?
            .or_else(|| {
                // Fallback to payloads map if head is not in storage yet
                self.read_storage.get_payload_by_block_hash(head_block_hash)
                    .map(|(b, _, _)| b)
            })
            .ok_or_else(|| RpcError::InvalidForkchoiceState(format!("Head block not found for payload building: {:?}", head_block_hash)))?;

        if attr.timestamp <= parent_block.header.timestamp {
            return Err(RpcError::InvalidPayloadAttributes("Invalid timestamp".to_string()));
        }

        let id = self.generate_payload_id(&head_block_hash, &attr).await;

        // Calculate base fee (simplified EIP-1559)
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });
        let base_fee_per_gas = self.consensus.calculate_next_base_fee(&parent_block.header, &chain_config);
        let blob_base_fee = self.consensus.calculate_next_blob_base_fee(&parent_block.header, &chain_config);

        let active_fork = wasix_eth_types::Hardfork::get_active_fork(&chain_config, parent_block.header.number + 1, attr.timestamp);
        let max_blobs_per_block = active_fork.blob_params(&chain_config).map(|p| p.max_blob_count as u32);

        // Build block logic
        let transactions = self.mempool.peek_best_transactions(
            parent_block.header.gas_limit, 
            U256::from(base_fee_per_gas.unwrap_or_default()),
            blob_base_fee.map(U256::from),
            max_blobs_per_block
        ).await;

        let execution = Arc::clone(&self.execution);
        let parent_header_clone = parent_block.header.clone();
        let attr_clone = attr.clone();
        let (finalized_block, receipts) = match tokio::task::spawn_blocking(move || {
            execution.execute_block_for_payload(
                transactions,
                &parent_header_clone,
                &attr_clone,
                base_fee_per_gas,
            )
        }).await {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => {
                warn!("[PayloadBuilder] Execution failed for payload building: {}", e);
                return Ok(None);
            }
            Err(e) => {
                error!("[PayloadBuilder] Payload building task panicked: {}", e);
                return Ok(None);
            }
        };

        let mut bundle = BlobsBundleV1::default();
        for tx in &finalized_block.body.transactions {
            if let Transaction::Eip4844(signed_tx) = tx {
                if let Some(hashes) = signed_tx.tx().blob_versioned_hashes() {
                    for hash in hashes {
                        if let Some((blob, commitment, proof)) = self.mempool.get_blob(*hash).await {
                            bundle.blobs.push(blob);
                            bundle.commitments.push(commitment);
                            bundle.proofs.push(proof);
                        } else {
                            return Err(RpcError::Internal(format!("Missing blob for transaction included in block: {:?}", signed_tx.hash())));
                        }
                    }
                }
            }
        }

        let write_storage = self.write_storage.clone();
        let finalized_block_clone = finalized_block.clone();
        let receipts_clone = receipts.clone();
        let blob_count = bundle.blobs.len() as u32;
        let id_clone = id.clone();

        match tokio::task::spawn_blocking(move || {
            write_storage.add_payload(id_clone, finalized_block_clone, receipts_clone, bundle)
        }).await {
            Ok(Ok(_)) => {},
            Ok(Err(e)) => {
                error!("[PayloadBuilder] Failed to persist payload: {}", e);
                return Ok(None);
            }
            Err(e) => {
                error!("[PayloadBuilder] Payload persistence task panicked: {}", e);
                return Ok(None);
            }
        }

        info!("[PayloadBuilder] Created payload_id={:?} for block_number={} with {} blobs", id, parent_block.header.number + 1, blob_count);

        Ok(Some(id))
    }

    pub async fn maybe_rebuild_payload(
        &self,
        payload_id: PayloadId,
    ) -> Result<()> {
        let (payload_block, _payload_receipts, payload_bundle) = match self.read_storage.get_payload(&payload_id) {
            Some(p) => p,
            None => return Ok(()),
        };

        let head_hash = payload_block.header.parent_hash;
        let old_blob_count = payload_bundle.blobs.len() as u32;

        let attr = PayloadAttributes {
            timestamp: payload_block.header.timestamp,
            prev_randao: payload_block.header.mix_hash,
            suggested_fee_recipient: payload_block.header.beneficiary,
            withdrawals: payload_block.body.withdrawals.as_ref().map(|w| w.0.clone()),
            parent_beacon_block_root: payload_block.header.parent_beacon_block_root,
        };

        // 1. Validate attributes (check if parent block still exists)
        let parent_block = self.read_storage.block_by_hash(head_hash).map_err(|e| RpcError::Internal(e.to_string()))?
            .or_else(|| {
                self.read_storage.get_payload_by_block_hash(head_hash)
                    .map(|(b, _, _)| b)
            });
        
        let parent_block = match parent_block {
            Some(b) => b,
            None => {
                debug!("[PayloadBuilder] Cannot rebuild payload {:?}: head block not found", payload_id);
                return Ok(());
            }
        };

        // 2. Compute base fees and max blobs
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });
        let base_fee_per_gas = self.consensus.calculate_next_base_fee(&parent_block.header, &chain_config);
        let blob_base_fee = self.consensus.calculate_next_blob_base_fee(&parent_block.header, &chain_config);

        let active_fork = wasix_eth_types::Hardfork::get_active_fork(&chain_config, parent_block.header.number + 1, attr.timestamp);
        let max_blobs_per_block = active_fork.blob_params(&chain_config).map(|p| p.max_blob_count as u32);

        // 3. Peek best transactions
        let transactions = self.mempool.peek_best_transactions(
            parent_block.header.gas_limit, 
            U256::from(base_fee_per_gas.unwrap_or_default()),
            blob_base_fee.map(U256::from),
            max_blobs_per_block
        ).await;

        // 4. Check new blob count without full execution first
        let mut new_blob_count = 0;
        for tx in &transactions {
            if let Transaction::Eip4844(signed_tx) = tx {
                if let Some(hashes) = signed_tx.tx().blob_versioned_hashes() {
                    new_blob_count += hashes.len() as u32;
                }
            }
        }

        if new_blob_count <= old_blob_count {
            // Check if there are any new transactions that might be better (e.g. higher tip)
            // Even if blob count is the same, HIVE tests might expect optimal selection.
            // But for now, we follow the requirement: "overwrite only when blob utilization improves"
            return Ok(());
        }

        tokio::task::yield_now().await;

        debug!("[PayloadBuilder] Rebuilding payload {:?}: blob count {} -> {}", payload_id, old_blob_count, new_blob_count);

        let write_storage = self.write_storage.clone();
        // 5. Re-run execution
        let execution = Arc::clone(&self.execution);
        let parent_header_clone = parent_block.header.clone();
        let attr_clone = attr.clone();
        let (finalized_block, receipts) = tokio::task::spawn_blocking(move || {
            execution.execute_block_for_payload(
                transactions,
                &parent_header_clone,
                &attr_clone,
                base_fee_per_gas,
            )
        }).await.map_err(|e| RpcError::Internal(format!("Payload rebuilding task panicked: {}", e)))?
        .map_err(|e| RpcError::Internal(e.to_string()))?;

        // 6. Collect blobs
        let mut bundle = BlobsBundleV1::default();
        for tx in &finalized_block.body.transactions {
            if let Transaction::Eip4844(signed_tx) = tx {
                if let Some(hashes) = signed_tx.tx().blob_versioned_hashes() {
                    for hash in hashes {
                        if let Some((blob, commitment, proof)) = self.mempool.get_blob(*hash).await {
                            bundle.blobs.push(blob);
                            bundle.commitments.push(commitment);
                            bundle.proofs.push(proof);
                        } else {
                            return Err(RpcError::Internal(format!("Missing blob for transaction included in rebuilt block: {:?}", signed_tx.hash())).into());
                        }
                    }
                }
            }
        }

        let finalized_block_clone = finalized_block.clone();
        let receipts_clone = receipts.clone();
        tokio::task::spawn_blocking(move || {
            write_storage.add_payload(payload_id, finalized_block_clone, receipts_clone, bundle)
        }).await.map_err(|e| RpcError::Internal(format!("Payload rebuilding task panicked: {}", e)))?
        .map_err(|e| RpcError::Internal(e.to_string()))?;

        Ok(())
    }

    pub fn get_payload(&self, payload_id: &PayloadId) -> RpcResult<(Block<Transaction>, Vec<Receipt>, BlobsBundleV1)> {
        self.read_storage.get_payload(payload_id)
            .ok_or_else(|| RpcError::BlockNotFound(wasix_eth_types::BlockId::Hash(B256::from_slice(&payload_id.0[..]).into())))
    }

    pub fn calculate_block_value(&self, block: &Block<Transaction>, receipts: &[Receipt]) -> U256 {
        let mut value = U256::ZERO;
        let base_fee = block.header.base_fee_per_gas.unwrap_or(0);
        
        // 1. Transaction fees (only the priority fee part/tips)
        let mut prev_cumulative_gas = 0;
        for (tx, receipt) in block.body.transactions.iter().zip(receipts.iter()) {
            let cumulative_gas = receipt.receipt.cumulative_gas_used;
            let gas_used = cumulative_gas.saturating_sub(prev_cumulative_gas);
            prev_cumulative_gas = cumulative_gas;

            let effective_gas_price = match tx {
                Transaction::Legacy(t) => t.tx().gas_price,
                Transaction::Eip2930(t) => t.tx().gas_price,
                Transaction::Eip1559(t) => {
                    let priority_fee = t.tx().max_priority_fee_per_gas;
                    let max_fee = t.tx().max_fee_per_gas;
                    base_fee as u128 + priority_fee.min(max_fee.saturating_sub(base_fee as u128))
                },
                Transaction::Eip4844(t) => {
                    let priority_fee = t.tx().max_priority_fee_per_gas().unwrap_or(0);
                    let max_fee = t.tx().max_fee_per_gas();
                    base_fee as u128 + priority_fee.min(max_fee.saturating_sub(base_fee as u128))
                },
                Transaction::Eip7702(t) => {
                    let priority_fee = t.tx().max_priority_fee_per_gas;
                    let max_fee = t.tx().max_fee_per_gas;
                    base_fee as u128 + priority_fee.min(max_fee.saturating_sub(base_fee as u128))
                }
            };
            
            let priority_fee_per_gas = effective_gas_price.saturating_sub(base_fee as u128);
            let tx_profit = U256::from(gas_used) * U256::from(priority_fee_per_gas);
            value += tx_profit;
        }
        
        value
    }
}