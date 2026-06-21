use futures_util::{future::BoxFuture, FutureExt};
use crate::ChainManager;
use crate::Consensus;
use alloy_rpc_types::RpcBlockHash;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::broadcast;
use wasix_eth_execution::execution_provider::ExecutionProvider;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::{BlockProvider, ChainProvider, HeaderProvider};
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{BlockWriter, HeaderWriter, TransactionWriter};
use wasix_eth_types::error::{RpcError, RpcResult};
use wasix_eth_types::{Block, BlockId, ChainConfig, PayloadStatus, PayloadStatusEnum, Transaction, B256, U256, proofs, Header};
use wasix_eth_utils::{debug, error, info};
use crate::engine::engine::EngineEvent;

#[derive(Clone)]
pub struct PayloadProcessor {
    pub orphan_pool: Arc<std::sync::RwLock<HashMap<B256, Vec<(Block<Transaction>, Option<Vec<B256>>, Option<B256>)>>>>,
    pub orphan_child_to_parent: Arc<std::sync::RwLock<HashMap<B256, B256>>>,
    pub processing_payloads: Arc<std::sync::RwLock<HashSet<B256>>>,
    pub consensus: Arc<dyn Consensus>,
    pub read_storage: DatabaseReadProvider,
    pub write_storage: DatabaseWriteProvider,
    pub execution: Arc<dyn ExecutionProvider>,
    pub chain: Arc<dyn ChainManager>,
    pub event_tx: broadcast::Sender<EngineEvent>,
}

impl PayloadProcessor {
    pub async fn new_payload_internal(
        &self, 
        block: Block<Transaction>, 
        expected_block_hash: B256, 
        expected_blob_versioned_hashes: Option<Vec<B256>>, 
        parent_beacon_block_root: Option<B256>
    ) -> RpcResult<PayloadStatus> {
        let actual_hash = block.header.hash_slow();
        if actual_hash != expected_block_hash {
            return Ok(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: "INVALID_BLOCK_HASH".to_string() },
                latest_valid_hash: None,
            });
        }

        // 1. Mark as processing to avoid concurrent imports of the same block
        {
            let mut processing = self.processing_payloads.write().unwrap();
            if processing.contains(&actual_hash) {
                debug!("[PayloadProcessor] Block {:?} is already being processed, skipping", actual_hash);
                return Ok(PayloadStatus {
                    status: PayloadStatusEnum::Syncing,
                    latest_valid_hash: None,
                });
            }
            processing.insert(actual_hash);
        }

        let result = self.new_payload_internal_inner(block, expected_block_hash, expected_blob_versioned_hashes, parent_beacon_block_root).await;

        if let Ok(ref status) = result {
            if status.status == PayloadStatusEnum::Valid {
                self.revalidate_dependent_payloads(actual_hash).await;
            }
        }

        {
            let mut processing = self.processing_payloads.write().unwrap();
            processing.remove(&actual_hash);
        }

        result
    }

    async fn get_header_from_anywhere(&self, hash: B256) -> Option<Header> {
        // 1. Check storage
        if let Ok(Some(header)) = self.read_storage.header(BlockId::Hash(hash.into())) {
            return Some(header);
        }
        
        // 2. Check payload map
        if let Some((payload, _, _)) = self.read_storage.get_payload_by_block_hash(hash) {
            return Some(payload.header.clone());
        }
        
        // 3. Check orphan pool
        {
            let pool = self.orphan_pool.read().unwrap();
            for children in pool.values() {
                for (block, _, _) in children {
                    if block.header.hash_slow() == hash {
                        return Some(block.header.clone());
                    }
                }
            }
        }
        
        None
    }


    pub async fn validate_parent_block(&self, parent_hash: B256) -> Option<PayloadStatus> {
        let is_genesis = self.read_storage.is_canonical(parent_hash).unwrap_or(false) &&
            self.read_storage.block_number(parent_hash).ok().flatten() == Some(0);

        // 1. Check if parent is known to be invalid
        if self.chain.is_invalid(parent_hash).await {
            let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
            debug!("[PayloadProcessor] Parent block {:?} is known to be invalid. Latest valid ancestor: {:?}", parent_hash, latest_valid);
            return Some(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: "Parent block is known to be invalid".to_string() },
                latest_valid_hash: latest_valid,
            });
        }

        // 2. Check if ANY ancestor is invalid by walking back
        let mut current_hash = parent_hash;
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        for i in 0..16 {
            if let Some(current_header) = self.get_header_from_anywhere(current_hash).await {
                let p_hash = current_header.parent_hash;

                // Check if parent is marked as invalid
                if self.chain.is_invalid(p_hash).await {
                    let latest_valid = self.chain.get_latest_valid_ancestor(p_hash).await;
                    debug!("[PayloadProcessor] Ancestor block {:?} at depth {} is known to be invalid. Latest valid ancestor: {:?}", p_hash, i, latest_valid);
                    self.chain.add_invalid_block(current_hash, p_hash).await;
                    
                    // Trigger recursive invalidation of orphans
                    let self_clone = self.clone();
                    tokio::spawn(async move {
                        self_clone.invalidate_descendants(current_hash).await;
                    });

                    return Some(PayloadStatus {
                        status: PayloadStatusEnum::Invalid { validation_error: "Ancestor block is known to be invalid".to_string() },
                        latest_valid_hash: latest_valid,
                    });
                }

                // Check if this block is invalid relative to its parent header if we can find it
                if let Some(p_header) = self.get_header_from_anywhere(p_hash).await {
                    if let Err(e) = self.consensus.validate_header(&current_header, &p_header, &chain_config) {
                        debug!("[PayloadProcessor] Ancestor block {:?} at depth {} failed header validation: {}. Latest valid ancestor: {:?}", current_hash, i, e, p_hash);
                        self.chain.add_invalid_block(current_hash, p_hash).await;
                        let latest_valid = self.chain.get_latest_valid_ancestor(p_hash).await;
                        return Some(PayloadStatus {
                            status: PayloadStatusEnum::Invalid { validation_error: format!("Ancestor header validation failed: {}", e) },
                            latest_valid_hash: latest_valid,
                        });
                    }
                }

                if p_hash == B256::ZERO || i == 15 { break; }
                current_hash = p_hash;
            } else {
                break;
            }
        }

        if self.read_storage.header(BlockId::Hash(RpcBlockHash::from(parent_hash))).ok().flatten().is_none() {
            if !is_genesis {
                debug!("[PayloadProcessor] Parent block not found in storage: requested_parent={:?}", parent_hash);
                
                self.chain.add_sync_target(parent_hash, None).await;
                if let Err(e) = self.chain.trigger_sync().await {
                    error!("[PayloadProcessor] Failed to trigger sync for missing parent: {}", e);
                }

                return Some(PayloadStatus {
                    status: PayloadStatusEnum::Syncing,
                    latest_valid_hash: None,
                });
            }
        }
        None
    }

    pub async fn new_payload_internal_inner(
        &self,
        block: Block<Transaction>,
        _expected_block_hash: B256,
        expected_blob_versioned_hashes: Option<Vec<B256>>,
        parent_beacon_block_root: Option<B256>
    ) -> RpcResult<PayloadStatus> {
        let actual_hash = block.header.hash_slow();
        let parent_hash = block.header.parent_hash;
        let block_number = block.header.number;

        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });
        // 2. Cancun validation
        if let Err(e) = self.consensus.validate_cancun(&block, expected_blob_versioned_hashes.as_deref()) {
            error!("[PayloadProcessor] Cancun validation failed: {}", e);
            // EIP-4844: "Client software SHOULD NOT permanently blacklist the block hash if it is rejected due to a mismatch between
            // expected_blob_versioned_hashes and the actual hashes in the block."

            // If the block is already known to be valid in storage, we MUST NOT mark it as invalid or remove it.
            // We just return INVALID for THIS call.
            if let Ok(Some(existing_header)) = self.read_storage.header(BlockId::Hash(actual_hash.into())) {
                if existing_header.hash_slow() == actual_hash {
                    let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
                    return Ok(PayloadStatus {
                        status: PayloadStatusEnum::Invalid { validation_error: e },
                        latest_valid_hash: latest_valid,
                    });
                }
            }

            // Only blacklist if we are sure about the parent.
            let parent_is_canonical = self.read_storage.is_canonical(parent_hash).unwrap_or(false);
            let parent_is_finalized = self.read_storage.forkchoice("finalized").ok().flatten() == Some(parent_hash);
            if parent_is_canonical || parent_is_finalized {
                self.chain.add_invalid_block(actual_hash, parent_hash).await;
            }

            let _ = self.write_storage.remove_payload_by_block_hash(actual_hash);
            let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
            return Ok(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: e },
                latest_valid_hash: latest_valid,
            });
        }
        // 1. Validate parent block
        if let Some(status) = self.validate_parent_block(parent_hash).await {
            if status.status != PayloadStatusEnum::Syncing && status.status != PayloadStatusEnum::Accepted {
                // If the parent is marked INVALID, we should only blacklist this block if the parent is canonical OR if it was already marked invalid by a canonical chain reorg.
                let parent_is_canonical = self.read_storage.is_canonical(parent_hash).unwrap_or(false);
                let parent_is_finalized = self.read_storage.forkchoice("finalized").ok().flatten() == Some(parent_hash);
                
                //if parent_is_canonical || parent_is_finalized {
                    self.chain.add_invalid_block(actual_hash, parent_hash).await;

                    // Recursive invalidation
                    let self_clone = self.clone();
                    tokio::spawn(async move {
                        self_clone.invalidate_descendants(actual_hash).await;
                    });
                //}

                let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
                return Ok(PayloadStatus {
                    status: status.status,
                    latest_valid_hash: latest_valid,
                });
            }

            if status.status == PayloadStatusEnum::Accepted || status.status == PayloadStatusEnum::Syncing {
                debug!("[PayloadProcessor] Parent unknown for block {:?}, buffering in orphan pool", actual_hash);
                {
                    let mut pool = self.orphan_pool.write().unwrap();

                    let children = pool.entry(parent_hash).or_default();
                    if !children.iter().any(|(b, _, _)| b.header.hash_slow() == actual_hash) {
                        children.push((block, expected_blob_versioned_hashes, parent_beacon_block_root));
                        self.orphan_child_to_parent.write().unwrap().insert(actual_hash, parent_hash);
                    }

                    if pool.len() > 64 {
                        if let Some(key) = pool.keys().next().cloned() {
                            if let Some(removed_children) = pool.remove(&key) {
                                for (child, _, _) in removed_children {
                                    self.orphan_child_to_parent.write().unwrap().remove(&child.header.hash_slow());
                                }
                            }
                        }
                    }
                }
                return Ok(status);
            }
        }



        // 3. Beacon root validation
        if let Err(e) = self.consensus.validate_parent_beacon_block_root(&block.header, parent_beacon_block_root) {
             error!("[PayloadProcessor] Beacon root validation failed: {}", e);
             
             // Same logic as Cancun validation: do not blacklist if already known.
             if let Ok(Some(existing_header)) = self.read_storage.header(BlockId::Hash(actual_hash.into())) {
                  if existing_header.hash_slow() == actual_hash {
                      let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
                      return Ok(PayloadStatus {
                          status: PayloadStatusEnum::Invalid { validation_error: e },
                          latest_valid_hash: latest_valid,
                      });
                  }
             }

             // Only blacklist if we are sure about the parent.
             let parent_is_canonical = self.read_storage.is_canonical(parent_hash).unwrap_or(false);
             let parent_is_finalized = self.read_storage.forkchoice("finalized").ok().flatten() == Some(parent_hash);
             if parent_is_canonical || parent_is_finalized {
                 self.chain.add_invalid_block(actual_hash, parent_hash).await;
             }
             
             let _ = self.write_storage.remove_payload_by_block_hash(actual_hash);
             let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
             return Ok(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: e },
                latest_valid_hash: latest_valid,
            });
        }

        // 5. Check if block is already known and validated
        if let Ok(Some(_)) = self.read_storage.block_body_by_hash(actual_hash) {
             // Even if known, we MUST ensure it passes Cancun and beacon root validation for THIS call.
             // We already did that above.
             return Ok(PayloadStatus {
                 status: PayloadStatusEnum::Valid,
                 latest_valid_hash: Some(actual_hash),
             });
        }

        // Before executing, check if parent header is available in storage.
        // We MUST NOT proceed to execution if the parent is only in the orphan pool or payload map,
        // because its state root hasn't been fully processed and committed to the database yet.
        let parent_header_in_storage = self.read_storage.header(BlockId::Hash(parent_hash.into())).ok().flatten();
        
        // If header not in storage, check if it's currently being processed or available elsewhere.
        let parent_header = if parent_header_in_storage.is_none() {
             if self.processing_payloads.read().unwrap().contains(&parent_hash) {
                  // If it's being processed, we can't get its header yet but we know it's coming.
                  // We return SYNCING and the CL will retry.
                  debug!("[PayloadProcessor] Parent block {:?} is currently being processed. Returning SYNCING.", parent_hash);
                  return Ok(PayloadStatus {
                      status: PayloadStatusEnum::Syncing,
                      latest_valid_hash: None,
                  });
             }

             // If parent is found in the orphan pool or as a pending payload, we must buffer this child and return ACCEPTED.
             if let Some(anywhere_header) = self.get_header_from_anywhere(parent_hash).await {
                  debug!("[PayloadProcessor] Parent block {:?} found in orphan pool or payload map. Buffering block {:?} and returning ACCEPTED.", parent_hash, actual_hash);
                  
                  // Even if we don't execute, we should validate the header if we have the parent header to return INVALID early if possible.
                  if let Err(e) = self.consensus.validate_header(&block.header, &anywhere_header, &chain_config) {
                      error!("[PayloadProcessor] Header validation failed for buffered block: {}", e);
                      self.chain.add_invalid_block(actual_hash, parent_hash).await;
                      let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
                      return Ok(PayloadStatus {
                          status: PayloadStatusEnum::Invalid { validation_error: e.to_string() },
                          latest_valid_hash: latest_valid,
                      });
                  }

                  {
                      let mut pool = self.orphan_pool.write().unwrap();
                      let children = pool.entry(parent_hash).or_default();
                      if !children.iter().any(|(b, _, _)| b.header.hash_slow() == actual_hash) {
                          children.push((block, expected_blob_versioned_hashes, parent_beacon_block_root));
                          self.orphan_child_to_parent.write().unwrap().insert(actual_hash, parent_hash);
                      }
                  }
                  return Ok(PayloadStatus {
                      status: PayloadStatusEnum::Accepted,
                      latest_valid_hash: None,
                  });
             }
             None
        } else {
             parent_header_in_storage
        };

        if parent_header.is_none() {
            debug!("[PayloadProcessor] Parent block {:?} header not found anywhere. Returning SYNCING to trigger discovery.", parent_hash);
            return Ok(PayloadStatus {
                status: PayloadStatusEnum::Syncing,
                latest_valid_hash: None,
            });
        }

        if let Some(parent) = &parent_header {
            if let Err(e) = self.consensus.validate_header(&block.header, parent, &chain_config) {
                error!("[PayloadProcessor] Header validation failed: {}", e);
                // Header validation is structural/independent of state.
                // We SHOULD blacklist if header validation fails, as it's inherent to the block.
                self.chain.add_invalid_block(actual_hash, parent_hash).await;
                
                let _ = self.write_storage.remove_payload_by_block_hash(actual_hash);
                let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
                return Ok(PayloadStatus {
                    status: PayloadStatusEnum::Invalid { validation_error: e },
                    latest_valid_hash: latest_valid,
                });
            }
        }

        // 5. Execute block
        let parent_state_root = parent_header.as_ref().map(|h| h.state_root);
        let execution = Arc::clone(&self.execution);
        let block_clone = block.clone();
        let exec_result = tokio::task::spawn_blocking(move || {
            execution.execute_block_with_state_root(block_clone, true, parent_state_root)
        }).await.map_err(|e| RpcError::Internal(format!("Execution task panicked: {}", e)))?;
        
        let (final_block, receipts) = match exec_result {
            Ok(res) => res,
            Err(e) => {
                error!("[PayloadProcessor] Execution failed for block {}: {}", block.header.number, e);
                
                // For execution errors (state dependent), we should be careful about blacklisting 
                // if we might be on a side-branch or syncing.
                // We blacklist if the parent is canonical OR if it's finalized.
                let parent_is_canonical = self.read_storage.is_canonical(parent_hash).unwrap_or(false);
                let parent_is_finalized = self.read_storage.forkchoice("finalized").ok().flatten() == Some(parent_hash);
                
                if parent_is_canonical || parent_is_finalized {
                    self.chain.add_invalid_block(actual_hash, parent_hash).await;
                }
                
                let _ = self.write_storage.remove_payload_by_block_hash(actual_hash);
                let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
                return Ok(PayloadStatus {
                    status: PayloadStatusEnum::Invalid { validation_error: e.to_string() },
                    latest_valid_hash: latest_valid,
                });
            }
        };

        // 6. Post-execution validation
        let cumulative_gas_used = receipts.last().map(|r| r.receipt.cumulative_gas_used).unwrap_or(0);
        let logs_bloom = {
            let mut bloom = wasix_eth_types::Bloom::ZERO;
            for receipt in &receipts {
                bloom.accrue_bloom(&receipt.logs_bloom);
            }
            bloom
        };
            if let Err(e) = self.consensus.validate_block_post_execution(
            &final_block,
            cumulative_gas_used,
            proofs::calculate_receipt_root(&receipts),
            logs_bloom,
            final_block.header.state_root
        ) {
            error!("[PayloadProcessor] Post-execution validation failed for block {}: {}", block_number, e);
            
            let parent_is_canonical = self.read_storage.is_canonical(parent_hash).unwrap_or(false);
            let parent_is_finalized = self.read_storage.forkchoice("finalized").ok().flatten() == Some(parent_hash);
            if parent_is_canonical || parent_is_finalized {
                self.chain.add_invalid_block(actual_hash, parent_hash).await;
            }
            
            let _ = self.write_storage.remove_payload_by_block_hash(actual_hash);
            let latest_valid = self.chain.get_latest_valid_ancestor(parent_hash).await;
            return Ok(PayloadStatus {
                status: PayloadStatusEnum::Invalid { validation_error: e },
                latest_valid_hash: latest_valid,
            });
        }

        // 7. Persist block
        let block_hash = final_block.header.hash_slow();
        let block_number = final_block.header.number;

        self.write_storage.insert_header(block_hash, final_block.header.clone()).map_err(|e| RpcError::Internal(e.to_string()))?;

        let parent_td = if block_number == 0 {
            U256::ZERO
        } else {
            match self.read_storage.header_td(final_block.header.parent_hash) {
                Ok(Some(td)) => td,
                Ok(None) => return Err(RpcError::Internal(format!("Parent TD not found for block {} (parent: {})", block_number, final_block.header.parent_hash))),
                Err(e) => return Err(RpcError::Internal(format!("Failed to retrieve parent TD for block {}: {}", block_number, e))),
            }
        };
        let td = parent_td + U256::from(final_block.header.difficulty);
        self.write_storage.insert_header_td(block_hash, td).map_err(|e| RpcError::Internal(e.to_string()))?;

        self.write_storage.insert_block_body(block_hash, block_number, final_block.body.clone()).map_err(|e| RpcError::Internal(e.to_string()))?;
        self.write_storage.insert_block_hash(block_hash, block_number).map_err(|e| RpcError::Internal(e.to_string()))?;

        // Persist receipts and transaction lookup
        for (i, tx) in final_block.body.transactions.iter().enumerate() {
            let tx_hash = *tx.hash();
            self.write_storage.insert_transaction(tx_hash, tx.clone()).map_err(|e| RpcError::Internal(e.to_string()))?;
            if let Some(receipt) = receipts.get(i) {
                self.write_storage.insert_receipt(block_hash, i as u64, receipt.clone()).map_err(|e| RpcError::Internal(e.to_string()))?;
            }
            self.write_storage.insert_transaction_lookup(tx_hash, block_hash, i as u64).map_err(|e| RpcError::Internal(e.to_string()))?;
        }

        info!("[PayloadProcessor] Successfully imported block {} (hash: {})", block_number, actual_hash);
        let _ = self.event_tx.send(EngineEvent::NewBlock(final_block.clone()));

        // If this block was previously marked as invalid (e.g. due to bad parameters in a previous call),
        // we should remove it from the invalid blocks list now that we've successfully validated and imported it.
        self.chain.remove_invalid_block(actual_hash).await;

        Ok(PayloadStatus {
            status: PayloadStatusEnum::Valid,
            latest_valid_hash: Some(actual_hash),
        })
    }

    pub fn revalidate_dependent_payloads(&self, initial_parent_hash: B256) -> BoxFuture<'_, ()> {
        async move {
            let mut parents_to_process = vec![initial_parent_hash];
            
            while let Some(parent_hash) = parents_to_process.pop() {
                let children = {
                    let mut pool = self.orphan_pool.write().unwrap();
                    pool.remove(&parent_hash)
                };
                
                if let Some(children_blocks) = children {
                    info!("[PayloadProcessor] Found {} dependent payloads for parent {:?}", children_blocks.len(), parent_hash);
                    for (block, expected_blobs, parent_beacon_root) in children_blocks {
                        let child_hash = block.header.hash_slow();
                        {
                            let mut child_to_parent = self.orphan_child_to_parent.write().unwrap();
                            child_to_parent.remove(&child_hash);
                        }
                        
                        // Mark as processing to avoid concurrent imports of the same block
                        {
                            let mut processing = self.processing_payloads.write().unwrap();
                            if processing.contains(&child_hash) {
                                debug!("[PayloadProcessor] Child block {:?} is already being processed, skipping revalidation", child_hash);
                                continue;
                            }
                            processing.insert(child_hash);
                        }

                        let result = self.new_payload_internal_inner(block, child_hash, expected_blobs, parent_beacon_root).await;

                        {
                            let mut processing = self.processing_payloads.write().unwrap();
                            processing.remove(&child_hash);
                        }

                        match result {
                            Ok(status) => {
                                if status.status == PayloadStatusEnum::Valid {
                                    parents_to_process.push(child_hash);
                                }
                            }
                            Err(e) => {
                                error!("[PayloadProcessor] Failed to revalidate dependent payload {:?}: {}", child_hash, e);
                            }
                        }
                    }
                }
            }
        }.boxed()
    }

    pub fn invalidate_descendants(&self, initial_invalid_hash: B256) -> BoxFuture<'_, ()> {
        async move {
            let mut parents_to_invalidate = vec![initial_invalid_hash];
            
            while let Some(parent_hash) = parents_to_invalidate.pop() {
                let children = {
                    let mut pool = self.orphan_pool.write().unwrap();
                    pool.remove(&parent_hash)
                };
                
                if let Some(children_blocks) = children {
                    debug!("[PayloadProcessor] Invalidating {} dependent payloads for invalid parent {:?}", children_blocks.len(), parent_hash);
                    for (block, _, _) in children_blocks {
                        let child_hash = block.header.hash_slow();
                        {
                            let mut child_to_parent = self.orphan_child_to_parent.write().unwrap();
                            child_to_parent.remove(&child_hash);
                        }
                        
                        // Mark as invalid in chain manager
                        self.chain.add_invalid_block(child_hash, parent_hash).await;
                        
                        // Recurse
                        parents_to_invalidate.push(child_hash);
                    }
                }
            }
        }.boxed()
    }
}