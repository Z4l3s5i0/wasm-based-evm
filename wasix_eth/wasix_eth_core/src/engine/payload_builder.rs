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

use std::collections::HashMap;
use std::sync::RwLock;

// Simple CancellationToken since we don't have tokio-util
#[derive(Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

#[derive(Clone)]
pub struct PayloadBuilder {
    pub consensus: Arc<dyn Consensus>,
    pub read_storage: DatabaseReadProvider,
    pub write_storage: DatabaseWriteProvider,
    pub execution: Arc<dyn ExecutionProvider>,
    pub mempool: Arc<dyn MempoolProvider>,
    pub cancel_tokens: Arc<RwLock<HashMap<PayloadId, CancellationToken>>>,
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

        // Comply with spec: SHOULD NOT restart if it already exists
        if self.read_storage.get_payload(&id).is_some() {
            return Ok(Some(id));
        }

        // 1. Build initial empty payload
        self.build_empty_payload(head_block_hash, attr.clone(), id.clone()).await?;

        // 2. Start continuous building background task
        let builder = self.clone();
        let token = CancellationToken::new();
        {
            let mut tokens = self.cancel_tokens.write().unwrap();
            if let Some(old_token) = tokens.insert(id.clone(), token.clone()) {
                let _ = old_token;
                old_token.cancel();
            }
        }

        tokio::spawn(async move {
            let start_time = std::time::Instant::now();
            let slot_duration = std::time::Duration::from_millis(12000); // SLOT_DURATION_MS
            
            while !token.is_cancelled() {
                if start_time.elapsed() >= slot_duration {
                    debug!("[PayloadBuilder] Stopping continuous building for {:?}: time limit reached", id);
                    break;
                }

                if let Err(e) = builder.maybe_rebuild_payload(id.clone()).await {
                    error!("[PayloadBuilder] Error during continuous building for {:?}: {:?}", id, e);
                }

                // Wait for a bit or until mempool change (simplified to 1s sleep for now)
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
                    _ = token.cancelled() => break,
                }
            }
            
            let mut tokens = builder.cancel_tokens.write().unwrap();
            if let Some(t) = tokens.get(&id) {
                let token_cancelled = t.is_cancelled();
                if token_cancelled == token.is_cancelled() {
                    tokens.remove(&id);
                }
            }
        });

        Ok(Some(id))
    }

    async fn build_empty_payload(
        &self,
        head_block_hash: B256,
        attr: PayloadAttributes,
        id: PayloadId,
    ) -> RpcResult<()> {
        let parent_block = self.read_storage.block_by_hash(head_block_hash).map_err(|e| RpcError::Internal(e.to_string()))?
            .or_else(|| {
                self.read_storage.get_payload_by_block_hash(head_block_hash)
                    .map(|(b, _, _)| b)
            })
            .ok_or_else(|| RpcError::InvalidForkchoiceState(format!("Head block not found for empty payload building: {:?}", head_block_hash)))?;

        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });
        let base_fee_per_gas = self.consensus.calculate_next_base_fee(&parent_block.header, &chain_config);

        let execution = Arc::clone(&self.execution);
        let parent_header_clone = parent_block.header.clone();
        let attr_clone = attr.clone();
        
        let (finalized_block, receipts) = match tokio::task::spawn_blocking(move || {
            execution.execute_block_for_payload(
                vec![], // Empty transaction set
                &parent_header_clone,
                &attr_clone,
                base_fee_per_gas,
            )
        }).await {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => {
                warn!("[PayloadBuilder] Execution failed for empty payload building: {}", e);
                return Err(RpcError::Internal(e.to_string()));
            }
            Err(e) => {
                error!("[PayloadBuilder] Empty payload building task panicked: {}", e);
                return Err(RpcError::Internal(e.to_string()));
            }
        };

        let bundle = BlobsBundleV1::default();
        let write_storage = self.write_storage.clone();
        let finalized_block_clone = finalized_block.clone();
        let receipts_clone = receipts.clone();
        let id_clone = id.clone();

        match tokio::task::spawn_blocking(move || {
            write_storage.add_payload(id_clone, finalized_block_clone, receipts_clone, bundle)
        }).await {
            Ok(Ok(_)) => {},
            Ok(Err(e)) => {
                error!("[PayloadBuilder] Failed to persist empty payload: {}", e);
                return Err(RpcError::Internal(e.to_string()));
            }
            Err(e) => {
                error!("[PayloadBuilder] Empty payload persistence task panicked: {}", e);
                return Err(RpcError::Internal(e.to_string()));
            }
        }

        info!("[PayloadBuilder] Created initial empty payload_id={:?} for block_number={}", id, parent_block.header.number + 1);
        Ok(())
    }

    pub async fn maybe_rebuild_payload(
        &self,
        payload_id: PayloadId,
    ) -> Result<()> {
        let (payload_block, payload_receipts, _payload_bundle) = match self.read_storage.get_payload(&payload_id) {
            Some(p) => p,
            None => return Ok(()),
        };

        let head_hash = payload_block.header.parent_hash;
        let old_value = self.calculate_block_value(&payload_block, &payload_receipts);

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

        if transactions.is_empty() && !payload_block.body.transactions.is_empty() {
             // Should not happen if we already have a payload with transactions, but good to be safe.
             return Ok(());
        }

        tokio::task::yield_now().await;

        let write_storage = self.write_storage.clone();
        // 4. Re-run execution
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

        // 5. Check if new block is better
        let new_value = self.calculate_block_value(&finalized_block, &receipts);
        if new_value <= old_value && !finalized_block.body.transactions.is_empty() && !payload_block.body.transactions.is_empty() {
            return Ok(());
        }
        
        // If the old one was empty and new one is not, or new one has higher value, replace it.
        if new_value <= old_value && !payload_block.body.transactions.is_empty() {
            return Ok(());
        }

        debug!("[PayloadBuilder] Rebuilding payload {:?}: value {} -> {}", payload_id, old_value, new_value);

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
        // Stop continuous building when payload is requested
        if let Some(token) = self.cancel_tokens.write().unwrap().remove(payload_id) {
            token.cancel();
        }

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