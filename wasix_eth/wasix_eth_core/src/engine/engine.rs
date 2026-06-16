use crate::account_manager::AccountManager;
use crate::chain_manager::ChainManager;
use crate::engine::payload_builder::PayloadBuilder;
use crate::engine::payload_processor::PayloadProcessor;
use crate::mempool::mempool_provider::MempoolProvider;
use crate::Consensus;
use std::sync::Arc;
use alloy_rlp::Encodable;
use alloy_rpc_types::RpcBlockHash;
use std::collections::{HashMap, HashSet};
use tokio::sync::broadcast;
use wasix_eth_execution::execution_provider::ExecutionProvider;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::BlockProvider;
use wasix_eth_storage::read_traits::ChainProvider;
use wasix_eth_storage::read_traits::HeaderProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{BlockWriter, ChangeSetWriter, HeaderWriter, TransactionWriter};
use wasix_eth_types::eip6110_utils::encode_deposit_request;
use wasix_eth_types::error::{RpcError, RpcResult};
use wasix_eth_types::sync::SyncProvider;
use wasix_eth_types::{Block, Header, Hardfork};
use wasix_eth_types::BlockId;
use wasix_eth_types::BlockNumberOrTag;
use wasix_eth_types::Bytes;
use wasix_eth_types::ChainConfig;
use wasix_eth_types::ExecutionPayloadBodyV1;
use wasix_eth_types::ExecutionPayloadEnvelopeV2;
use wasix_eth_types::ExecutionPayloadEnvelopeV3;
use wasix_eth_types::ExecutionPayloadEnvelopeV4;
use wasix_eth_types::ExecutionPayloadV1;
use wasix_eth_types::ExecutionPayloadV3;
use wasix_eth_types::ExecutionPayloadV4;
use wasix_eth_types::ForkchoiceState;
use wasix_eth_types::ForkchoiceUpdated;
use wasix_eth_types::PayloadAttributes;
use wasix_eth_types::PayloadId;
use wasix_eth_types::PayloadStatus;
use wasix_eth_types::PayloadStatusEnum;
use wasix_eth_types::Receipt;
use wasix_eth_types::Transaction;
use wasix_eth_types::B256;
use wasix_eth_types::U256;
use wasix_eth_types::{async_trait};
use wasix_eth_types::{BlobAndProofV1, BlobAndProofV2, BlobsBundleV1, Decodable2718};
use wasix_eth_types::eip4895::Withdrawal;
use wasix_eth_utils::engine_mapper::EngineMapper;
use wasix_eth_utils::metrics::CURRENT_HEAD_BLOCK;
use wasix_eth_utils::warn;
use wasix_eth_utils::{debug, error, info};
use crate::engine::api::RPCEngine;
use crate::engine::forkchoice_validator::ForkchoiceValidator;
use crate::mempool::listener::MempoolListener;

#[derive(Clone, Debug)]
pub enum EngineEvent {
    NewBlock(Block<Transaction>),
    NewTransaction(Transaction),
}

#[derive(Clone)]
pub struct Engine {
    pub read_storage: DatabaseReadProvider,
    pub write_storage: DatabaseWriteProvider,
    pub execution: Arc<dyn ExecutionProvider>,
    pub account_manager: Arc<AccountManager>,
    pub chain: Arc<dyn ChainManager>,
    pub mempool: Arc<dyn MempoolProvider>,
    pub event_tx: broadcast::Sender<EngineEvent>,
    pub consensus: Arc<dyn Consensus>,
    pub payload_builder: PayloadBuilder,
    pub payload_processor: PayloadProcessor,
    pub rpc_engine: Arc<RPCEngine>,
    pub forkchoice_validator: ForkchoiceValidator,
    pub mempool_listener: Arc<MempoolListener>,
}




#[async_trait]
impl SyncProvider for Engine {
    async fn status(&self) -> wasix_eth_types::SyncStatus {
        self.chain.sync_status().await
    }
    async fn trigger_sync(&self) -> wasix_eth_types::Result<()> {
        self.chain.trigger_sync().await
    }
    async fn has_block(&self, hash: B256) -> bool {
        self.chain.has_block(hash).await
    }
    async fn process_gossip_block(&self, block: Block<Transaction>, _td: U256) -> wasix_eth_types::Result<()> {
        self.import_block(block).await
    }

    async fn process_gossip_transactions(&self, txs: Vec<Transaction>) -> wasix_eth_types::Result<()> {
        for tx in txs {
            let _ = self.rpc_engine.submit_transaction(tx).await;
        }
        Ok(())
    }

    async fn process_pooled_transactions(&self, txs: Vec<wasix_eth_types::TxPooledEnvelope>) -> wasix_eth_types::Result<()> {
        for tx in txs {
            let mut data = Vec::new();
            tx.encode(&mut data);
            let _ = self.rpc_engine.import_pooled_transaction(tx, data).await;
        }
        Ok(())
    }

    async fn handle_announced_pooled_transactions(&self, _peer_id: String, _hashes: Vec<B256>) -> wasix_eth_types::Result<()> {
        // Engine doesn't have a downloader, it relies on the sync controller to fetch
        Ok(())
    }
}

impl Engine {
    pub fn new(
        read_storage: DatabaseReadProvider,
        write_storage: DatabaseWriteProvider,
        execution: Arc<dyn ExecutionProvider>,
        account_manager: Arc<AccountManager>,
        chain: Arc<dyn ChainManager>,
        mempool: Arc<dyn MempoolProvider>,
        event_tx: broadcast::Sender<EngineEvent>,
        consensus: Arc<dyn Consensus>,
        rpc_engine: Arc<RPCEngine>,
        forkchoice_validator: ForkchoiceValidator,
        mempool_listener: Arc<MempoolListener>,
    ) -> Self {
        let orphan_pool = Arc::new(std::sync::RwLock::new(HashMap::new()));
        let orphan_child_to_parent = Arc::new(std::sync::RwLock::new(HashMap::new()));
        let processing_payloads = Arc::new(std::sync::RwLock::new(HashSet::new()));
        
        let engine = Self {
            read_storage: read_storage.clone(),
            write_storage: write_storage.clone(),
            execution: execution.clone(),
            account_manager,
            chain: chain.clone(),
            mempool: mempool.clone(),
            event_tx: event_tx.clone(),
            consensus: consensus.clone(),
            payload_builder: PayloadBuilder { 
                consensus: consensus.clone(),
                read_storage: read_storage.clone(),
                write_storage: write_storage.clone(),
                execution: execution.clone(),
                mempool: mempool.clone(),
            },
            payload_processor: PayloadProcessor { 
                orphan_pool: orphan_pool.clone(), 
                orphan_child_to_parent: orphan_child_to_parent.clone(), 
                processing_payloads: processing_payloads.clone(), 
                consensus: consensus.clone(),
                read_storage: read_storage.clone(),
                write_storage: write_storage.clone(),
                execution: execution.clone(),
                chain: chain.clone(),
                event_tx: event_tx.clone(),
            },
            rpc_engine,
            forkchoice_validator,
            mempool_listener,
        };

        // Spawn rebuild listener
        let mut event_rx = event_tx.subscribe();
        let engine_clone = engine.clone();
        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                if let EngineEvent::NewTransaction(tx) = event {
                    if tx.is_eip4844() {
                        let is_sidecar_available = if let Transaction::Eip4844(signed_tx) = &tx {
                            signed_tx.tx().sidecar().as_ref().map(|s| s.is_eip4844()).unwrap_or(false)
                        } else {
                            false
                        };
                        
                        if is_sidecar_available {
                            let head_hash = engine_clone.read_storage.forkchoice("head").ok().flatten().unwrap_or_default();
                            let active_ids = engine_clone.read_storage.all_payload_ids();
                            for payload_id in active_ids {
                                if let Some((payload_block, _, _bundle)) = engine_clone.read_storage.get_payload(&payload_id) {
                                    if payload_block.header.parent_hash == head_hash {
                                        let builder = engine_clone.payload_builder.clone();
                                        tokio::spawn(async move {
                                            let _ = builder.maybe_rebuild_payload(payload_id).await;
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        engine
    }

    // ------ Engine methods  ------

    pub async fn get_payload_bodies_by_hash(&self, hashes: Vec<B256>) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        let mut bodies = Vec::new();
        for hash in hashes {
            let body = self.read_storage.block_body_by_hash(hash).map(|body_opt| {
                body_opt.and_then(|body| {
                    let header = self.read_storage.header(BlockId::Hash(hash.into())).ok().flatten();
                    header.map(|header| {
                        let block = Block { header, body: body.into() };
                        EngineMapper::to_execution_payload_body_v1(&block)
                    })
                })
            }).map_err(|e| RpcError::Internal(e.to_string()))?;
            bodies.push(body);
        }
        Ok(bodies)
    }

    pub async fn get_payload_bodies_by_range(&self, start: u64, count: u64) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        if count == 0 {
            return Err(RpcError::InvalidParamsCode("count must be at least 1".to_string()));
        }
        if start == 0 {
            return Err(RpcError::InvalidParamsCode("start must be at least 1".to_string()));
        }
        let head_number = self.rpc_engine.latest_block_number().await?;

        if start > head_number {
            return Ok(Vec::new());
        }

        let actual_count = std::cmp::min(count, head_number - start + 1);
        let mut bodies = Vec::with_capacity(actual_count as usize);
        for i in 0..actual_count {
            let number = start + i;
            let body = self.read_storage.block_body(number)
                .map(|body_opt| {
                    body_opt.and_then(|body| {
                        let header = self.read_storage.header(BlockId::Number(BlockNumberOrTag::Number(number))).ok().flatten();
                        header.map(|header| {
                            let block = Block { header, body: body.into() };
                            EngineMapper::to_execution_payload_body_v1(&block)
                        })
                    })
                })
                .map_err(|e| RpcError::Internal(e.to_string()))?;
            bodies.push(body);
        }
        Ok(bodies)
    }

    pub async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1> {
        let (block, _, _) = self.payload_builder.get_payload(&payload_id)?;
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_default();
        let fork = Hardfork::get_active_fork(&chain_config, block.header.number, block.header.timestamp);

        // engine_getPayloadV1 must be used for forks before Cancun
        if fork >= Hardfork::Cancun {
            return Err(RpcError::UnsupportedFork("engine_getPayloadV1 must be used for forks before Cancun".to_string()));
        }

        Ok(EngineMapper::to_execution_payload_v1(&block, &chain_config))
    }
    
    pub async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadEnvelopeV2> {
        let (block, receipts, _) = self.payload_builder.get_payload(&payload_id)?;
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_default();
        let fork = Hardfork::get_active_fork(&chain_config, block.header.number, block.header.timestamp);

        // engine_getPayloadV2 must be used for forks before Cancun
        if fork >= Hardfork::Cancun {
            return Err(RpcError::UnsupportedFork("engine_getPayloadV2 must be used for forks before Cancun".to_string()));
        }

        let execution_payload = EngineMapper::to_execution_payload_v2(&block, &chain_config);
        let block_value = self.payload_builder.calculate_block_value(&block, &receipts);

        Ok(EngineMapper::to_execution_payload_envelope_v2(execution_payload, block_value, fork))
    }

    pub async fn get_payload_v3(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadEnvelopeV3> {
        let (block, receipts, bundle) = self.payload_builder.get_payload(&payload_id)?;
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_default();
        let fork = Hardfork::get_active_fork(&chain_config, block.header.number, block.header.timestamp);

        // engine_getPayloadV3 and above must be used for Cancun and above
        if fork < Hardfork::Cancun {
            return Err(RpcError::UnsupportedFork("engine_getPayloadV3 and above must be used for Cancun and above".to_string()));
        }

        let execution_payload = EngineMapper::to_execution_payload_v3(&block, &chain_config);
        let block_value = self.payload_builder.calculate_block_value(&block, &receipts);
        
        Ok(EngineMapper::to_execution_payload_envelope_v3(execution_payload, block_value, bundle))
    }

    pub async fn get_payload_v4(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadEnvelopeV4> {
        let (block, receipts, bundle) = self.payload_builder.get_payload(&payload_id)?;
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_default();

        let execution_payload = EngineMapper::to_execution_payload_v4(&block, &chain_config);
        let block_value = self.payload_builder.calculate_block_value(&block, &receipts);

        let fork = Hardfork::get_active_fork(&chain_config, block.header.number, block.header.timestamp);
        let mut execution_requests = Vec::new();
        if fork >= Hardfork::Prague {
            // Recalculate execution requests for Prague
            let provider = wasix_eth_execution::execution_provider::EthExecutionProvider::new(
                self.read_storage.clone(),
                self.write_storage.clone(),
            );
            
            // Collect deposits (type 0)
            let deposits = provider.collect_deposits(&receipts);
            for deposit in deposits {
                use alloy_rlp::Encodable;
                let mut deposit_buf = Vec::new();
                encode_deposit_request(&deposit, &mut deposit_buf);

                let mut request_buf = Vec::new();
                0u8.encode(&mut request_buf);
                request_buf.extend_from_slice(&deposit_buf);

                execution_requests.push(request_buf.into());
            }

            // For EIP-7002 withdrawal requests (type 1), we re-run the system call to retrieve them.
            if let Ok(withdrawals) = provider.collect_withdrawal_requests(&block.header) {
                for withdrawal in withdrawals {
                    execution_requests.push(withdrawal.into());
                }
            } else {
                warn!("[Engine] Failed to retrieve EIP-7002 withdrawal requests for block {}", block.header.number);
            }

            // For EIP-7251 consolidation requests (type 2), we re-run the system call to retrieve them.
            if let Ok(consolidations) = provider.collect_consolidation_requests(&block.header) {
                for consolidation in consolidations {
                    execution_requests.push(consolidation.into());
                }
            } else {
                warn!("[Engine] Failed to retrieve EIP-7251 consolidation requests for block {}", block.header.number);
            }
        }

        Ok(EngineMapper::to_execution_payload_envelope_v4(execution_payload, block_value, bundle, execution_requests))
    }

    pub async fn get_blobs_v1(&self, versioned_hashes: Vec<B256>) -> RpcResult<Vec<Option<BlobAndProofV1>>> {
        let mut result = Vec::with_capacity(versioned_hashes.len());
        for hash in versioned_hashes {
            let blob_and_proof = self.mempool.get_blob(hash).await.map(|(blob, _, proof)| {
                BlobAndProofV1 { blob: Box::new(blob), proof }
            });
            result.push(blob_and_proof);
        }
        Ok(result)
    }

    pub async fn get_blobs_v2(&self, versioned_hashes: Vec<B256>) -> RpcResult<Option<Vec<BlobAndProofV2>>> {
        let mut result = Vec::with_capacity(versioned_hashes.len());
        for hash in versioned_hashes {
            if let Some((blob, _, proof)) = self.mempool.get_blob(hash).await {
                result.push(BlobAndProofV2 {
                    blob: Box::new(blob),
                    proofs: vec![proof],
                });
            } else {
                return Ok(None);
            }
        }
        Ok(Some(result))
    }

    pub async fn get_blobs_v3(&self, versioned_hashes: Vec<B256>) -> RpcResult<Option<Vec<Option<BlobAndProofV2>>>> {
        let mut result = Vec::with_capacity(versioned_hashes.len());
        for hash in versioned_hashes {
            result.push(self.mempool.get_blob(hash).await.map(|(blob, _, proof)| BlobAndProofV2 {
                blob: Box::new(blob),
                proofs: vec![proof],
            }));
        }
        Ok(Some(result))
    }

    pub async fn get_blobs_v4(&self, _versioned_hashes: Vec<B256>, _indices_bitarray: wasix_eth_types::B128) -> RpcResult<Option<serde_json::Value>> {
        // TODO
        // V4 is more complex and depends on EIP-7594.
        // For now, return None as placeholder but with correct signature.
        Ok(None)
    }

    pub async fn forkchoice_updated(&self, forkchoice_state: ForkchoiceState,
                                    payload_attributes: Option<PayloadAttributes>,
                                    version: u8) -> RpcResult<ForkchoiceUpdated> {

        debug!("[Engine] forkchoice_updated: head={:?} safe={:?} finalized={:?} version={}", 
            forkchoice_state.head_block_hash, forkchoice_state.safe_block_hash, forkchoice_state.finalized_block_hash, version);

        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        // 0. Check if head block is known before ancestry validation
        let head_status = self.determine_payload_status(forkchoice_state.head_block_hash).await;

        // If headBlockHash is unknown or invalid, return status immediately and don't validate ancestry
        if head_status.status != PayloadStatusEnum::Valid {
            debug!("[Engine] Head block {:?} status is {:?}, returning immediately", forkchoice_state.head_block_hash, head_status.status);
            return Ok(ForkchoiceUpdated {
                payload_status: head_status,
                payload_id: None,
            });
        }

        // Now that head is known (and valid/syncing is handled by status below), validate safe and finalized
        if forkchoice_state.head_block_hash != B256::ZERO {
            // Check if head block actually exists in our storage or payload map before checking ancestry.
            let head_header = self.read_storage.header(BlockId::Hash(RpcBlockHash::from(forkchoice_state.head_block_hash))).ok().flatten()
                .or_else(|| self.read_storage.get_payload_by_block_hash(forkchoice_state.head_block_hash).map(|(p, _, _)| p.header.clone()))
                .or_else(|| {
                    // Manual check for genesis if header lookup by hash failed
                    let genesis_hash = self.read_storage.block_hash(0).unwrap_or(None);
                    if genesis_hash == Some(forkchoice_state.head_block_hash) {
                         self.read_storage.header(BlockId::Number(BlockNumberOrTag::Number(0))).ok().flatten()
                    } else {
                        None
                    }
                });
            
            if let Some(header) = head_header {
                debug!("[Engine] Validating ancestry for head block #{}", header.number);
                if let Some(value) = self.forkchoice_validator.check_safe_block(forkchoice_state, &header).await {
                    return value;
                }
                if let Some(value) = self.forkchoice_validator.check_finalized_block(forkchoice_state, header).await {
                    return value;
                }
            } else {
                if let Some(value) = self.forkchoice_validator.check_header_without_headheader(forkchoice_state) {
                    return value;
                }
            }
        }

        // Save current forkchoice state for potential rollback
        let (old_head, old_safe, old_finalized) = self.get_current_forkchoice_state()?;

        // Update forkchoice in storage
        self.write_storage.update_forkchoice(forkchoice_state.head_block_hash, Some(forkchoice_state.safe_block_hash), Some(forkchoice_state.finalized_block_hash))
            .map_err(|e| RpcError::Internal(e.to_string()))?;

        // 1. We already determined head_status above
        let status = head_status.clone();

        if status.status != PayloadStatusEnum::Valid && status.status != PayloadStatusEnum::Syncing {
             // Rollback forkchoice in storage if the new head is explicitly invalid
             let _ = self.write_storage.update_forkchoice(old_head, old_safe, old_finalized);
             return Ok(ForkchoiceUpdated {
                payload_status: status,
                payload_id: None,
            });
        }

        // Update metrics and handle canonical chain events if VALID or SYNCING
        if status.status == PayloadStatusEnum::Valid || status.status == PayloadStatusEnum::Syncing {
            if let Ok(Some(header)) = self.read_storage.header(BlockId::Hash(RpcBlockHash::from(forkchoice_state.head_block_hash))) {
                if status.status == PayloadStatusEnum::Valid {
                    // 1. Identify newly canonical blocks and handle reorgs
                    match self.chain.resolve_reorg(old_head, forkchoice_state.head_block_hash).await {
                        Ok(context) => {
                            // 2. Handle reorgs FIRST: revert to common ancestor
                            if context.is_reorg {
                                // Find common ancestor height
                                let common_ancestor_height = self.read_storage.header(BlockId::Hash(RpcBlockHash::from(context.common_ancestor_hash)))
                                    .ok()
                                    .flatten()
                                    .map(|h| h.number)
                                    .unwrap_or(0);
                                
                                info!("[Engine] Reorg detected! Reverting chain to height {} (common ancestor: {:?})", common_ancestor_height, context.common_ancestor_hash);
                                
                                // Physically roll back state to common ancestor
                                if let Err(e) = self.revert_to_height(common_ancestor_height).await {
                                    error!("[Engine] Failed to revert to height {}: {}", common_ancestor_height, e);
                                    return Err(RpcError::Internal(format!("Revert failed: {}", e)));
                                }

                                self.handle_reorgs_for_mempool(forkchoice_state, old_head, context.common_ancestor_hash).await;
                            }

                            // 3. Mark the new branch as canonical SECOND
                            if !context.new_canonical_blocks.is_empty() {
                                if let Err(e) = self.chain.mark_branch_canonical(&context.new_canonical_blocks).await {
                                    error!("[Engine] Failed to mark branch canonical: {}", e);
                                    return Err(RpcError::Internal(format!("Canonical marking failed: {}", e)));
                                }

                                // 4. Emit CanonicalBlock events for the newly canonical blocks
                                for block in context.new_canonical_blocks {
                                    // let _ = self.event_tx.send(EngineEvent::CanonicalBlock(block));
                                    self.mempool_listener.handle_canonical_block(block).await;
                                }
                            } else if old_head != forkchoice_state.head_block_hash {
                                // Edge case: resolve_reorg returned empty blocks but heads differ
                                // This can happen if the new head is already marked canonical somehow
                                debug!("[Engine] Resolve reorg returned empty new blocks but heads differ ({:?} -> {:?})", old_head, forkchoice_state.head_block_hash);
                            }
                        }
                        Err(e) => {
                            error!("[Engine] Failed to resolve reorg from {:?} to {:?}: {}", old_head, forkchoice_state.head_block_hash, e);
                            return Err(RpcError::Internal(format!("Reorg resolution failed: {}", e)));
                        }
                    }
                }
                debug!("[Engine] Updating head to #{} hash {:?}", header.number, forkchoice_state.head_block_hash);
                self.update_head_block(forkchoice_state, &header).await;
            }
        }

        if let Some(attr) = &payload_attributes {
            let head_block = self.read_storage.block_by_hash(forkchoice_state.head_block_hash).ok().flatten()
                .or_else(|| {
                    self.read_storage.get_payload_by_block_hash(forkchoice_state.head_block_hash)
                        .map(|(b, _, _)| b)
                });
            
            if let Some(parent) = head_block {
                if attr.timestamp <= parent.header.timestamp {
                    return Err(RpcError::InvalidPayloadAttributes("Invalid timestamp".to_string()));
                }
            }

            if let Some(value) = Self::fork_validation(version, &chain_config, attr) {
                return value;
            }
        }

        // 2. Build payload if requested and status is VALID (or SYNCING)
        let (status, payload_id) = if status.status == PayloadStatusEnum::Valid || (status.status == PayloadStatusEnum::Syncing && payload_attributes.is_some()) {
            if let Some(attr) = payload_attributes {
                match self.build_new_payload(forkchoice_state.head_block_hash, attr, &status).await {
                    Ok(id) => (status, id),
                    Err(e) => {
                        // Rollback forkchoice in storage on error
                        let _ = self.write_storage.update_forkchoice(old_head, old_safe, old_finalized);
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

    fn fork_validation(version: u8, chain_config: &ChainConfig, attr: &PayloadAttributes) -> Option<RpcResult<ForkchoiceUpdated>> {
        let fork = Hardfork::get_active_fork(&chain_config, 0, attr.timestamp); // block number unknown here, using 0 but timestamp should be enough for Cancun

        // engine_forkchoiceUpdatedV3 and above must be used for Cancun and above
        if version >= 3 && fork < Hardfork::Cancun {
            if attr.parent_beacon_block_root.is_some() {
                 return Some(Err(RpcError::UnsupportedFork("engine_forkchoiceUpdatedV3 and above must be used for Cancun and above".to_string())));
            } else {
                 return Some(Err(RpcError::InvalidPayloadAttributes("engine_forkchoiceUpdatedV3 and above must be used for Cancun and above".to_string())));
            }
        }
        // engine_forkchoiceUpdatedV2 and below must be used for forks before Cancun
        if version < 3 && fork >= Hardfork::Cancun {
            if attr.parent_beacon_block_root.is_some() {
                 return Some(Err(RpcError::InvalidPayloadAttributes("engine_forkchoiceUpdatedV2 and below must be used for forks before Cancun".to_string())));
            } else {
                 return Some(Err(RpcError::UnsupportedFork("engine_forkchoiceUpdatedV2 and below must be used for forks before Cancun".to_string())));
            }
        }

        // Shanghai validation: withdrawals must be present if and only if Shanghai is active
        if fork >= Hardfork::Shanghai {
            if attr.withdrawals.is_none() {
                return Some(Err(RpcError::InvalidPayloadAttributes("missing withdrawals in payload attributes".to_string())));
            }
        } else {
            if attr.withdrawals.is_some() {
                return Some(Err(RpcError::InvalidPayloadAttributes("unexpected withdrawals in payload attributes".to_string())));
            }
        }

        // Cancun validation: parentBeaconBlockRoot must be present if and only if Cancun is active
        if fork >= Hardfork::Cancun {
            if attr.parent_beacon_block_root.is_none() {
                return Some(Err(RpcError::InvalidPayloadAttributes("missing parentBeaconBlockRoot in payload attributes".to_string())));
            }
        } else {
            if attr.parent_beacon_block_root.is_some() {
                return Some(Err(RpcError::InvalidPayloadAttributes("unexpected parentBeaconBlockRoot in payload attributes".to_string())));
            }
        }
        None
    }

    async fn handle_reorgs_for_mempool(&self, forkchoice_state: ForkchoiceState, old_head: B256, common_ancestor_hash: B256) {
        info!("[Engine] Reorg detected! Old head: {:?}, New head: {:?}, Common ancestor: {:?}",
                            old_head, forkchoice_state.head_block_hash, common_ancestor_hash);

        // First, emit Reorg event to signal listener to potentially clear/revalidate
        // let _ = self.event_tx.send(EngineEvent::Reorg {
        //     hash: forkchoice_state.head_block_hash,
        //     number: header.number
        // });
        self.mempool_listener.handle_reorg(forkchoice_state.head_block_hash).await;

        let mut discarded_hash = old_head;
        let mut all_discarded_txs = Vec::new();
        while discarded_hash != B256::ZERO && discarded_hash != common_ancestor_hash {
            if let Ok(Some(block)) = self.read_storage.block_by_hash(discarded_hash) {
                info!("[Engine] Re-adding {} transactions from discarded block #{} hash {:?}",
                                    block.body.transactions.len(), block.header.number, discarded_hash);
                for tx in block.body.transactions {
                    all_discarded_txs.push(tx);
                }
                discarded_hash = block.header.parent_hash;
            } else {
                break;
            }
        }

        // Re-add in reverse order (oldest discarded first)
        for tx in all_discarded_txs.into_iter().rev() {
            let _ = self.event_tx.send(EngineEvent::NewTransaction(tx));
        }
    }

    async fn update_head_block(&self, forkchoice_state: ForkchoiceState, header: &Header) {
        self.chain.set_head_block(forkchoice_state.head_block_hash, header.number).await;
        let _ = self.write_storage.update_forkchoice(forkchoice_state.head_block_hash, Some(forkchoice_state.safe_block_hash), Some(forkchoice_state.finalized_block_hash));
        CURRENT_HEAD_BLOCK.set(header.number as f64);
    }

    fn get_current_forkchoice_state(&self) -> Result<(B256, Option<B256>, Option<B256>), RpcError> {
        let (old_head, old_safe, old_finalized) = {
            let head = self.read_storage.forkchoice("head").unwrap_or(None).unwrap_or_default();
            let safe = self.read_storage.forkchoice("safe").unwrap_or(None);
            let finalized = self.read_storage.forkchoice("finalized").unwrap_or(None);

            if head == B256::ZERO {
                // Fallback to latest canonical block if forkchoice table is empty
                let latest = self.read_storage.latest_block_number().map_err(|e| RpcError::Internal(e.to_string()))?;
                let head = if let Some(n) = latest {
                    self.read_storage.block_hash(n).map_err(|e| RpcError::Internal(e.to_string()))?.unwrap_or_default()
                } else {
                    B256::ZERO
                };
                (head, Some(head), Some(head))
            } else {
                (head, safe, finalized)
            }
        };
        Ok((old_head, old_safe, old_finalized))
    }

    pub async fn import_block(&self, block: Block<Transaction>) -> wasix_eth_types::Result<()> {
        let block_num = block.header.number;
        let block_hash = block.header.hash_slow();

        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        // 1. Validate body roots
        if let Err(e) = self.consensus.validate_body(&block, &chain_config) {
            error!("[Engine] Body validation failed for block {}: {}", block_num, e);
            self.chain.add_invalid_block(block_hash, block.header.parent_hash).await;
            self.payload_processor.revalidate_dependent_payloads(block_hash).await;
            return Err(anyhow::anyhow!("Body validation failed: {}", e));
        }

        // 2. Validate parent
        if let Some(status) = self.validate_parent_block(block.header.parent_hash).await {
            match status.status {
                PayloadStatusEnum::Valid => {},
                PayloadStatusEnum::Invalid { .. } => {
                    self.chain.add_invalid_block(block_hash, block.header.parent_hash).await;
                    self.revalidate_dependent_payloads(block_hash).await;
                    return Err(anyhow::anyhow!("Parent block validation failed: {:?}", status.status));
                }
                _ => return Err(anyhow::anyhow!("Parent block validation failed: {:?}", status.status)),
            }
        }

        // 2.5 Cancun validation (internal consistency only as we are in sync mode)
        if let Err(e) = self.consensus.validate_cancun(&block, None) {
            error!("[Engine] Cancun validation failed for block {}: {}", block_num, e);
            self.chain.add_invalid_block(block_hash, block.header.parent_hash).await;
            self.revalidate_dependent_payloads(block_hash).await;
            return Err(anyhow::anyhow!("Cancun validation failed: {}", e));
        }

        // 2.6 Beacon root validation
        if let Err(e) = self.consensus.validate_parent_beacon_block_root(&block.header, block.header.parent_beacon_block_root) {
            error!("[Engine] Beacon root validation failed for block {}: {}", block_num, e);
            self.chain.add_invalid_block(block_hash, block.header.parent_hash).await;
            self.revalidate_dependent_payloads(block_hash).await;
            return Err(anyhow::anyhow!("Beacon root validation failed: {}", e));
        }

        let parent_header = self.read_storage.header(BlockId::Hash(RpcBlockHash::from(block.header.parent_hash))).ok().flatten();
        if let Some(parent) = &parent_header {
            if let Err(e) = self.consensus.validate_header(&block.header, parent, &chain_config) {
                error!("[Engine] Header validation failed for block {}: {}", block_num, e);
                self.chain.add_invalid_block(block_hash, block.header.parent_hash).await;
                self.revalidate_dependent_payloads(block_hash).await;
                return Err(anyhow::anyhow!("Header validation failed: {}", e));
            }
        }

        // 3. Execute block
        let parent_state_root = parent_header.as_ref().map(|h| h.state_root);

        let batch = self.write_storage.begin_batch()?;

        match self.execution.execute_block_with_batch(block.clone(), &batch, parent_state_root) {
            Ok((final_block, receipts)) => {
                // 3.5 Post-execution validation
                if let Err(e) = self.consensus.validate_block_post_execution(
                    &block, 
                    final_block.header.gas_used, 
                    final_block.header.receipts_root, 
                    final_block.header.logs_bloom, 
                    final_block.header.state_root
                ) {
                    error!("[Engine] Post-execution validation failed for block {}: {}", block_num, e);
                    self.chain.add_invalid_block(block_hash, block.header.parent_hash).await;
                    self.revalidate_dependent_payloads(block_hash).await;
                    return Err(anyhow::anyhow!("Post-execution validation failed: {}", e));
                }

                // 4. Persist block atomically
                let parent_td = if block_num == 0 {
                    U256::ZERO
                } else {
                    match self.read_storage.header_td(block.header.parent_hash) {
                        Ok(Some(td)) => td,
                        Ok(None) => return Err(anyhow::anyhow!("Parent TD not found for block {} (parent: {})", block_num, block.header.parent_hash)),
                        Err(e) => return Err(anyhow::anyhow!("Failed to retrieve parent TD for block {}: {}", block_num, e)),
                    }
                };
                let td = parent_td + block.header.difficulty;

                batch.insert_header(block_hash, block.header.clone())?;
                batch.insert_header_td(block_hash, td)?;
                batch.insert_header_number(block_hash, block_num)?;
                batch.insert_block_hash(block_hash, block_num)?;
                batch.insert_block_body(block_hash, block_num, block.body.clone())?;
                
                // Persist ChangeSets
                let account_changes = batch.collect_account_changes();
                let storage_changes = batch.collect_storage_changes();
                batch.insert_account_change_set(block_num, account_changes)?;
                batch.insert_storage_change_set(block_num, storage_changes)?;

                // Persist receipts and transaction lookup
                for (i, tx) in block.body.transactions.iter().enumerate() {
                    let tx_hash = *tx.hash();
                    batch.insert_transaction(tx_hash, tx.clone())?;
                    if let Some(receipt) = receipts.get(i) {
                        batch.insert_receipt(block_hash, i as u64, receipt.clone())?;
                    }
                    batch.insert_transaction_lookup(tx_hash, block_hash, i as u64)?;
                }

                batch.commit()?;

                // Emit event for new block
                let _ = self.event_tx.send(EngineEvent::NewBlock(block));
                info!("[Engine] Successfully imported block {} (hash: {})", block_num, block_hash);
                
                // Trigger revalidation of any payloads waiting for this block
                self.revalidate_dependent_payloads(block_hash).await;

                Ok(())
            }
            Err(e) => {
                self.chain.add_invalid_block(block_hash, block.header.parent_hash).await;
                // Trigger revalidation for descendants (they will become INVALID)
                self.revalidate_dependent_payloads(block_hash).await;
                Err(anyhow::anyhow!("Execution failed for block {}: {}", block_num, e))
            }
        }
    }

    pub async fn new_payload_v3(
        &self,
        payload: ExecutionPayloadV3,
        expected_blob_versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
    ) -> RpcResult<PayloadStatus> {
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        let timestamp = payload.payload_inner.payload_inner.timestamp;
        let block_number = payload.payload_inner.payload_inner.block_number;
        let fork = Hardfork::get_active_fork(&chain_config, block_number, timestamp);
        if fork < Hardfork::Cancun {
            return Err(RpcError::UnsupportedFork("unsupported fork".to_string()));
        }

        let transactions = self.decode_transactions(&payload.payload_inner.payload_inner.transactions).await?;
        let block = EngineMapper::payload_v3_to_block(&payload, transactions, parent_beacon_block_root, &chain_config);
        let expected_block_hash = payload.payload_inner.payload_inner.block_hash;
        
        self.new_payload_internal(block, expected_block_hash, Some(expected_blob_versioned_hashes), Some(parent_beacon_block_root)).await
    }

    pub async fn new_payload_v4(
        &self,
        payload: ExecutionPayloadV4,
        expected_blob_versioned_hashes: Vec<B256>,
        parent_beacon_block_root: B256,
        execution_requests: Vec<Bytes>,
    ) -> RpcResult<PayloadStatus> {
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        let timestamp = payload.payload_inner.payload_inner.payload_inner.timestamp;
        let block_number = payload.payload_inner.payload_inner.payload_inner.block_number;
        let fork = Hardfork::get_active_fork(&chain_config, block_number, timestamp);
        if fork < Hardfork::Prague {
            return Err(RpcError::UnsupportedFork("unsupported fork".to_string()));
        }

        let transactions = self.decode_transactions(&payload.payload_inner.payload_inner.payload_inner.transactions).await?;
        let block = EngineMapper::payload_v4_to_block(&payload, transactions, parent_beacon_block_root, execution_requests, &chain_config);
        let expected_block_hash = payload.payload_inner.payload_inner.payload_inner.block_hash;
        
        self.new_payload_internal(block, expected_block_hash, Some(expected_blob_versioned_hashes), Some(parent_beacon_block_root)).await
    }

    pub async fn new_payload(&self, payload_v1: ExecutionPayloadV1, withdrawals: Option<Vec<alloy_rpc_types::Withdrawal>>) -> RpcResult<PayloadStatus> {
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        if let Some(value) = Self::new_payload_fork_validation(&payload_v1, &withdrawals, &chain_config) {
            return value;
        }

        let transactions = self.decode_transactions(&payload_v1.transactions).await?;
        let block = EngineMapper::payload_v1_to_block(&payload_v1, transactions, withdrawals, &chain_config, None, None, None);
        let expected_block_hash = payload_v1.block_hash;
        self.new_payload_internal(block, expected_block_hash, None, None).await
    }

    fn new_payload_fork_validation(payload_v1: &ExecutionPayloadV1, withdrawals: &Option<Vec<Withdrawal>>, chain_config: &ChainConfig) -> Option<RpcResult<PayloadStatus>> {
        let fork = Hardfork::get_active_fork(&chain_config, payload_v1.block_number, payload_v1.timestamp);
        debug!("[Engine] new_payload: block={}, timestamp={}, fork={:?}, withdrawals_is_some={}", payload_v1.block_number, payload_v1.timestamp, fork, withdrawals.is_some());

        // Shanghai validation: withdrawals must be present if and only if Shanghai is active
        if fork >= Hardfork::Shanghai {
            if withdrawals.is_none() {
                error!("[Engine] Missing withdrawals in post-Shanghai payload (fork={:?}, block={}, timestamp={})", fork, payload_v1.block_number, payload_v1.timestamp);
                return Some(Err(RpcError::InvalidParamsCode("missing withdrawals".to_string())));
            }
        } else if withdrawals.is_some() {
            error!("[Engine] Unexpected withdrawals in pre-Shanghai payload (fork={:?}, block={}, timestamp={})", fork, payload_v1.block_number, payload_v1.timestamp);
            return Some(Err(RpcError::InvalidParamsCode("unexpected withdrawals".to_string())));
        }

        if fork >= Hardfork::Cancun {
            debug!("[Engine] new_payload: allowing V1/V2 for Cancun block");
            return Some(Err(RpcError::UnsupportedFork("unsupported fork".to_string())));
        }
        None
    }

    async fn new_payload_internal(
        &self,
        block: Block<Transaction>,
        expected_block_hash: B256,
        expected_blob_versioned_hashes: Option<Vec<B256>>,
        parent_beacon_block_root: Option<B256>,
    ) -> RpcResult<PayloadStatus> {
        self.payload_processor.new_payload_internal(block, expected_block_hash, expected_blob_versioned_hashes, parent_beacon_block_root).await
    }

    // --- helper methods engine calls ---
    pub async fn get_payload(&self, payload_id: &PayloadId) -> RpcResult<(Block<Transaction>, Vec<Receipt>, BlobsBundleV1)> {
        self.payload_builder.get_payload(payload_id)
    }

    async fn build_new_payload(
        &self,
        head_block_hash: B256,
        attr: PayloadAttributes,
        status: &PayloadStatus,
    ) -> RpcResult<Option<PayloadId>> {
        self.payload_builder.build_new_payload(head_block_hash, attr, status).await
    }
    
    pub async fn revert_to_height(&self, height: u64) -> wasix_eth_types::Result<()> {
        self.chain.revert_to_height(height).await?;

        // Side effects: Update metrics and emit event
        let (head_hash, head_number) = self.chain.head_block().await;
        wasix_eth_utils::metrics::CURRENT_HEAD_BLOCK.set(head_number as f64);

        if self.read_storage.header(BlockId::Hash(head_hash.into())).is_ok() {
            self.mempool_listener.handle_reorg(head_hash).await;
            // let _ = self.event_tx.send(EngineEvent::Reorg { hash: head_hash, number: head_number });
            debug!("[Engine] Reorg handled for height {}", height);
        }

        Ok(())
    }
    async fn decode_transactions(&self, txs: &[Bytes]) -> RpcResult<Vec<Transaction>> {
        let mut transactions = Vec::new();
        for tx_bytes in txs {
            let tx = Transaction::decode_2718(&mut &tx_bytes[..])
                .map_err(|e| RpcError::InvalidParams(format!("Failed to decode transaction: {}", e)))?;
            transactions.push(tx);
        }
        Ok(transactions)
    }

    async fn determine_payload_status(&self, head_block_hash: B256) -> PayloadStatus {
        self.chain.determine_payload_status(head_block_hash).await
    }
    
    async fn validate_parent_block(&self, parent_hash: B256) -> Option<PayloadStatus> {
        self.payload_processor.validate_parent_block(parent_hash).await
    }

    pub async fn invalidate_descendants(&self, initial_invalid_hash: B256) {
        self.payload_processor.invalidate_descendants(initial_invalid_hash).await
    }

    async fn revalidate_dependent_payloads(&self, initial_parent_hash: B256) {
        self.payload_processor.revalidate_dependent_payloads(initial_parent_hash).await
    }
}



