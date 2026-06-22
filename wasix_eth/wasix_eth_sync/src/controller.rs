use crate::downloader::Downloader;
use crate::processor::BlockProcessor;
use std::sync::Arc;
use alloy_primitives::{B256, U256};
use alloy_rpc_types::SyncInfo;
use alloy_rpc_types::engine::ForkchoiceState;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::{BlockProvider, ChainProvider, TransactionProvider};
use wasix_eth_core::{ChainManager, InvalidationReason};
use wasix_eth_core::mempool::mempool_provider::MempoolProvider;
use wasix_eth_types::sync::{SyncProvider, PeerProvider};
use wasix_eth_types::{async_trait, SyncStatus, Transaction, Block, Hardfork, ChainConfig};
use wasix_eth_utils::metrics::SYNC_STATUS;
use wasix_eth_utils::{debug, error, info};

pub struct SyncController {
    read_storage: DatabaseReadProvider,
    mempool: Arc<dyn MempoolProvider>,
    downloader: Downloader,
    processor: BlockProcessor,
    chain_manager: Arc<dyn ChainManager>,
    sync_lock: tokio::sync::Mutex<()>,
}

impl SyncController {
    pub fn new(
        read_storage: DatabaseReadProvider,
        peer_provider: Arc<dyn PeerProvider>,
        processor: BlockProcessor,
        chain_manager: Arc<dyn ChainManager>,
        mempool: Arc<dyn MempoolProvider>,
    ) -> Self {
        Self {
            read_storage: read_storage.clone(),
            mempool,
            downloader: Downloader::new(peer_provider),
            processor,
            chain_manager,
            sync_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn start(&self) {
        info!("[Sync] Starting synchronization controller...");
        loop {
            if let Err(e) = self.sync_step().await {
                error!("[Sync] Sync step failed: {}", e);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    async fn sync_step(&self) -> anyhow::Result<()> {
        let _lock = match self.sync_lock.try_lock() {
            Ok(lock) => lock,
            Err(_) => {
                debug!("[Sync] Sync already in progress, skipping step");
                return Ok(());
            }
        };

        let (_local_hash, local_height) = self.chain_manager.head_block().await;
        let local_td = self.chain_manager.total_difficulty().await;

        if let Some((target_hash, affinity_peer)) = self.chain_manager.pop_sync_target().await {
            debug!("[Sync] Attempting to sync specific target {:?} (affinity: {:?})", target_hash, affinity_peer);
            
            let mut sync_peer = None;
            if let Some(peer_id) = affinity_peer.as_ref() {
                if let Some(_session) = self.downloader.peer_provider.get_session(peer_id).await {
                    sync_peer = Some(peer_id.clone());
                }
            }
            
            if sync_peer.is_none() {
                if let Some((best_peer_id, _, _)) = self.downloader.get_best_peer().await {
                    sync_peer = Some(best_peer_id);
                }
            }

            if let Some(peer_id) = sync_peer {
                if let Err(e) = self.fetch_ancestors(&peer_id, target_hash, target_hash).await {
                     error!("[Sync] Failed to fetch target ancestor {:?}: {}", target_hash, e);
                }
            } else {
                debug!("[Sync] No peers found for target {:?}, triggering broadened discovery", target_hash);
                // Put it back
                self.chain_manager.add_sync_target(target_hash, affinity_peer).await;
            }
        }

        let best_peer = self.downloader.get_best_peer().await;
        if let Some((best_peer_id, best_td, best_height)) = best_peer {
            let is_better = if best_td > local_td {
                true
            } else if best_td == local_td && best_td > U256::ZERO {
                best_height > local_height
            } else if best_td == U256::ZERO {
                // For Eth69, assume better if height is higher
                best_height > local_height
            } else {
                false
            };

            if is_better {
                debug!("[Sync] Found better peer {} with TD {} height {} (local TD: {}, height: {})", 
                    best_peer_id, best_td, best_height, local_td, local_height);
                
                self.sync_range(&best_peer_id, local_height + 1).await?;
            } else {
                self.chain_manager.set_sync_status(SyncStatus::None).await;
                SYNC_STATUS.set(1.0); // 1: synced
            }
        } else {
            debug!("[Sync] No peers found with better TD or height (local TD: {}, local height: {})", local_td, local_height);
        }

        Ok(())
    }

    async fn sync_range(&self, peer_id: &str, start: u64) -> anyhow::Result<()> {
        const BATCH_SIZE: u64 = 64;
        debug!("[Sync] Starting sync range from header {}", start);
        let local_height = self.read_storage.latest_block_number()?.unwrap_or(0);
        
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        // 1. Download headers in bulk
        let headers = match self.downloader.download_headers(peer_id, start, BATCH_SIZE).await {
            Ok(h) => h,
            Err(e) => {
                error!("[Sync] Failed to download headers from {}: {}", peer_id, e);
                self.chain_manager.set_sync_status(SyncStatus::None).await;
                return Err(e);
            }
        };

        if headers.is_empty() {
            debug!("[Sync] No new headers from peer {}", peer_id);
            return Ok(());
        }

        let end = headers.last().unwrap().number;

        self.chain_manager.set_sync_status(SyncStatus::Info(Box::new(SyncInfo {
            current_block: U256::from(start),
            highest_block: U256::from(end),
            starting_block: U256::from(local_height),
            warp_chunks_amount: None,
            warp_chunks_processed: None,
            stages: None,
        }))).await;
        SYNC_STATUS.set(0.0);

        // 2. Validate header chain (basic link check)
        for i in 1..headers.len() {
            if headers[i].parent_hash != headers[i-1].hash_slow() {
                self.chain_manager.set_sync_status(SyncStatus::None).await;
                return Err(anyhow::anyhow!("Downloaded header chain is not contiguous at height {}", headers[i].number));
            }
        }

        // 3. Download bodies in bulk
        let hashes: Vec<B256> = headers.iter().map(|h| h.hash_slow()).collect();
        let bodies = match self.downloader.download_bodies(peer_id, hashes.clone()).await {
            Ok(b) => b,
            Err(e) => {
                error!("[Sync] Failed to download bodies from {}: {}", peer_id, e);
                self.chain_manager.set_sync_status(SyncStatus::None).await;
                return Err(e);
            }
        };

        if bodies.len() != headers.len() {
            self.chain_manager.set_sync_status(SyncStatus::None).await;
            return Err(anyhow::anyhow!("Downloaded {} bodies for {} headers", bodies.len(), headers.len()));
        }

        // 4. Process blocks
        let mut last_imported_block = None;
        let mut last_imported_timestamp = 0;
        for (header, body) in headers.into_iter().zip(bodies.into_iter()) {
            let block_num = header.number;
            let block_timestamp = header.timestamp;
            let block = wasix_eth_types::Block { header, body };
            let block_hash = block.header.hash_slow();

            // Check if parent exists in storage
            let parent_hash = block.header.parent_hash;
            let parent_exists = self.read_storage.block_by_hash(parent_hash)?.is_some();

            if !parent_exists && block_num > 0 {
                debug!("[Sync] Parent block {:?} for {} missing, fetching ancestors", parent_hash, block_num);
                self.chain_manager.add_sync_target(parent_hash, Some(peer_id.to_string())).await;
                if let Err(e) = self.fetch_ancestors(peer_id, parent_hash, block_hash).await {
                    error!("[Sync] Failed to fetch ancestors: {}", e);
                    self.chain_manager.add_invalid_block(block_hash, parent_hash, InvalidationReason::Hard).await;
                    self.chain_manager.set_sync_status(SyncStatus::None).await;
                    return Err(e);
                }
            }

            if let Err(e) = self.processor.process_block(block.clone()).await {
                error!("[Sync] Failed to process block {}: {}", block_num, e);
                // Mark the block as invalid in ChainManager
                self.chain_manager.add_invalid_block(block_hash, block.header.parent_hash, InvalidationReason::Hard).await;
                
                // Trigger recursive invalidation of orphans for this invalid block
                self.processor.invalidate_descendants(block_hash).await;

                self.chain_manager.set_sync_status(SyncStatus::None).await;
                return Err(e);
            }
            last_imported_block = Some((block_hash, block_num));
            last_imported_timestamp = block_timestamp;
        }
        
        // 5. Update forkchoice to the latest imported block
        if let Some((hash, number)) = last_imported_block {
            let state = ForkchoiceState {
                head_block_hash: hash,
                safe_block_hash: hash,
                finalized_block_hash: B256::ZERO,
            };
            
            let fork = Hardfork::get_active_fork(&chain_config, number, last_imported_timestamp);
            let version = if fork >= Hardfork::Cancun { 3 } else { 2 };

            match self.processor.engine.forkchoice_updated(state, None, version).await {
                Ok(_) => info!("[Sync] Successfully updated forkchoice (V{}) to block {} ({})", version, number, hash),
                Err(e) => error!("[Sync] Failed to update forkchoice after sync range: {}", e),
            }
        }

        self.chain_manager.set_sync_status(SyncStatus::None).await;
        Ok(())
    }

    async fn fetch_ancestors(&self, peer_id: &str, hash: B256, requested_head: B256) -> anyhow::Result<()> {
        let mut current_hash = hash;
        let mut to_process = Vec::new();

        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        while current_hash != B256::ZERO {
            // Check if we already have it
            if self.read_storage.block_by_hash(current_hash)?.is_some() {
                break;
            }

            // Check if it's known to be invalid
            if let Some(reason) = self.chain_manager.get_invalidation_reason(current_hash).await {
                if reason == InvalidationReason::Hard {
                    self.chain_manager.add_invalid_block(requested_head, current_hash, InvalidationReason::Hard).await;
                    return Err(anyhow::anyhow!("Ancestor block {:?} is known to be invalid (Hard)", current_hash));
                } else {
                    debug!("[Sync] Ancestor block {:?} was marked as Soft invalid, attempting retry", current_hash);
                    self.chain_manager.remove_invalid_block(current_hash).await;
                }
            }

            debug!("[Sync] Fetching ancestor block {:?}", current_hash);
            let block = self.downloader.download_block_by_hash(peer_id, current_hash).await?;
            let parent_hash = block.header.parent_hash;
            to_process.push(block);
            current_hash = parent_hash;

            if to_process.len() > 100 {
                debug!("[Sync] Ancestor chain too long, processing current batch");
                break;
            }
        }

        // Process in reverse order (oldest to newest)
        let mut last_processed_hash = None;
        let mut last_processed_num = 0;
        let mut last_processed_timestamp = 0;
        
        for block in to_process.into_iter().rev() {
            let block_num = block.header.number;
            let block_hash = block.header.hash_slow();
            let parent_hash = block.header.parent_hash;
            let block_timestamp = block.header.timestamp;

            if let Err(e) = self.processor.process_block(block).await {
                error!("[Sync] Failed to process ancestor block {}: {}", block_num, e);
                // Mark block as invalid
                self.chain_manager.add_invalid_block(block_hash, parent_hash, InvalidationReason::Hard).await;
                
                // Immediately mark the requested head as invalid too
                if requested_head != block_hash {
                    self.chain_manager.add_invalid_block(requested_head, block_hash, InvalidationReason::Hard).await;
                }

                // Trigger recursive invalidation of orphans for this invalid ancestor
                self.processor.invalidate_descendants(block_hash).await;

                return Err(anyhow::anyhow!("Ancestor block processing failed: {}", e));
            }
            last_processed_hash = Some(block_hash);
            last_processed_num = block_num;
            last_processed_timestamp = block_timestamp;
        }

        // Update forkchoice if we processed any blocks
        if let Some(hash) = last_processed_hash {
            let state = ForkchoiceState {
                head_block_hash: hash,
                safe_block_hash: hash,
                finalized_block_hash: B256::ZERO,
            };
            
            let fork = Hardfork::get_active_fork(&chain_config, last_processed_num, last_processed_timestamp);
            let version = if fork >= Hardfork::Cancun { 3 } else { 2 };

            if let Err(e) = self.processor.engine.forkchoice_updated(state, None, version).await {
                error!("[Sync] Failed to update forkchoice (V{}) after fetching ancestors to block {}: {}", version, last_processed_num, e);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl SyncProvider for SyncController {
    async fn status(&self) -> SyncStatus {
        self.chain_manager.sync_status().await
    }

    async fn trigger_sync(&self) -> anyhow::Result<()> {
        info!("[Sync] Manual sync trigger received");
        let result = self.sync_step().await;
        if result.is_ok() {
            debug!("[Sync] Manual sync step completed successfully");
        } else {
            error!("[Sync] Manual sync step failed: {:?}", result.as_ref().err());
        }
        result
    }

    async fn has_block(&self, hash: B256) -> bool {
        self.chain_manager.has_block(hash).await
    }

    async fn process_gossip_block(&self, block: Block<Transaction>, _td: U256) -> anyhow::Result<()> {
        let block_hash = block.header.hash_slow();
        let block_num = block.header.number;
        let block_timestamp = block.header.timestamp;
        
        let chain_config = self.read_storage.chain_config().ok().flatten().unwrap_or_else(|| ChainConfig {
            chain_id: self.read_storage.chain_id().unwrap_or(1),
            ..Default::default()
        });

        self.processor.process_block(block).await?;
        
        // Update forkchoice for gossip block
        let state = ForkchoiceState {
            head_block_hash: block_hash,
            safe_block_hash: block_hash,
            finalized_block_hash: B256::ZERO,
        };
        
        let fork = Hardfork::get_active_fork(&chain_config, block_num, block_timestamp);
        let version = if fork >= Hardfork::Cancun { 3 } else { 2 };

        if let Err(e) = self.processor.engine.forkchoice_updated(state, None, version).await {
            error!("[Sync] Failed to update forkchoice (V{}) for gossip block {} ({}): {}", version, block_num, block_hash, e);
        }
        
        Ok(())
    }

    async fn process_gossip_transactions(&self, txs: Vec<Transaction>) -> anyhow::Result<()> {
        for tx in txs {
            let _ = self.processor.process_transaction(tx).await;
        }
        Ok(())
    }

    async fn process_pooled_transactions(&self, txs: Vec<wasix_eth_types::TxPooledEnvelope>) -> wasix_eth_types::Result<()> {
        for tx in txs {
            let _ = self.processor.process_pooled_transaction(tx).await;
        }
        Ok(())
    }

    async fn handle_announced_pooled_transactions(&self, peer_id: String, hashes: Vec<B256>) -> wasix_eth_types::Result<()> {
        let mut hashes_to_download = Vec::with_capacity(hashes.len());
        for hash in hashes {
            // Skip if already in mempool
            if self.mempool.get_transaction(hash).await.is_some() {
                continue;
            }
            // Skip if already on-chain
            if self.read_storage.transaction(hash).unwrap_or_default().is_some() {
                continue;
            }
            hashes_to_download.push(hash);
        }

        if hashes_to_download.is_empty() {
            return Ok(());
        }

        let txs = self.downloader.download_pooled_transactions(&peer_id, hashes_to_download).await
            .map_err(|e| wasix_eth_types::error::RpcError::Internal(format!("Failed to download pooled transactions: {}", e)))?;
        
        info!("[Sync] Downloaded {} pooled transactions from {}", txs.len(), peer_id);
        self.process_pooled_transactions(txs).await
    }
}
