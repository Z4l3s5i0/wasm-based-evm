use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::storage::storage::InMemoryStorage;
use crate::storage::traits::StateProvider;
use crate::p2p::peer_manager::PeerManager;
use crate::mempool::Mempool;
use crate::{info, error, debug};
use alloy_consensus::transaction::SignerRecoverable;
use alloy_primitives::{U256};
use alloy_rpc_types::{SyncStatus, SyncInfo};
use crate::evm::executor::Executor;
use crate::misc::metrics::SYNC_STATUS;
use crate::sync::downloader::Downloader;
use crate::sync::processor::BlockProcessor;

pub struct SyncController {
    storage: Arc<RwLock<InMemoryStorage>>,
    mempool: Arc<RwLock<Mempool>>,
    downloader: Downloader,
    processor: BlockProcessor,
    is_syncing: AtomicBool,
    current_block: AtomicU64,
    highest_block: AtomicU64,
}

impl SyncController {
    pub fn new(
        storage: Arc<RwLock<InMemoryStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        peer_manager: Arc<PeerManager>,
        executor: Arc<Executor>
    ) -> Self {
        Self {
            storage: storage.clone(),
            mempool: mempool.clone(),
            downloader: Downloader::new(peer_manager),
            processor: BlockProcessor::new(storage, executor, mempool),
            is_syncing: AtomicBool::new(false),
            current_block: AtomicU64::new(0),
            highest_block: AtomicU64::new(0),
        }
    }

    pub async fn start(&self) {
        info!("[Sync] Starting synchronization controller...");
        loop {
            if let Err(e) = self.sync_step().await {
                error!("[Sync] Sync step failed: {}", e);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
    }

    pub async fn trigger_sync(&self) -> anyhow::Result<()> {
        info!("[Sync] Manual sync trigger received");
        let result = self.sync_step().await;
        if result.is_ok() {
            debug!("[Sync] Manual sync step completed successfully");
        } else {
            error!("[Sync] Manual sync step failed: {:?}", result.as_ref().err());
        }
        result
    }

    pub async fn has_block(&self, hash: alloy_primitives::B256) -> bool {
        let storage = self.storage.read().await;
        storage.get_block_by_hash(hash).is_some()
    }

    pub async fn status(&self) -> SyncStatus {
        if self.is_syncing.load(Ordering::SeqCst) {
            let starting_block = {
                let storage_read = self.storage.read().await;
                storage_read.get_latest_block_number()
            };
            SyncStatus::Info(Box::new(SyncInfo {
                current_block: U256::from(self.current_block.load(Ordering::SeqCst)),
                highest_block: U256::from(self.highest_block.load(Ordering::SeqCst)),
                starting_block: U256::from(starting_block),
                warp_chunks_amount: None,
                warp_chunks_processed: None,
                stages: None,
            }))
        } else {
            SyncStatus::None
        }
    }

    async fn sync_step(&self) -> anyhow::Result<()> {
        let (local_height, local_hash) = {
            let storage_read = self.storage.read().await;
            let height = storage_read.get_latest_block_number();
            let hash = storage_read.get_block_hash(height).unwrap_or_default();
            (height, hash)
        };

        if let Some((best_peer_url, best_height)) = self.downloader.get_best_peer(local_height).await {
            self.highest_block.store(best_height, Ordering::SeqCst);
            
            if best_height > local_height {
                self.is_syncing.store(true, Ordering::SeqCst);
                SYNC_STATUS.set(0.0); // 0: syncing
            } else {
                self.is_syncing.store(false, Ordering::SeqCst);
                SYNC_STATUS.set(1.0); // 1: synced
            }

            // Check for divergence
            if local_height > 0 {
                match self.downloader.get_block_hash(&best_peer_url, local_height).await {
                    Ok(Some(peer_hash)) if peer_hash != local_hash => {
                        info!("[Sync] Divergence detected at height {}. Local hash: {:?}, Peer hash: {:?}", local_height, local_hash, peer_hash);
                        // Find common ancestor
                        let mut ancestor_height = local_height - 1;
                        while ancestor_height > 0 {
                            let local_ancestor_hash = self.storage.read().await.get_block_hash(ancestor_height).unwrap_or_default();
                            match self.downloader.get_block_hash(&best_peer_url, ancestor_height).await {
                                Ok(Some(peer_ancestor_hash)) if peer_ancestor_hash == local_ancestor_hash => {
                                    debug!("[Sync] Common ancestor found at height {}", ancestor_height);
                                    break;
                                }
                                _ => ancestor_height -= 1,
                            }
                        }
                        
                        // Reorg
                        let reverted_txs = {
                            let mut storage_write = self.storage.write().await;
                            storage_write.revert_to_height(ancestor_height)
                        };
                        
                        debug!("[Sync] Reverted {} blocks. Re-adding {} transactions to mempool", local_height - ancestor_height, reverted_txs.len());
                        {
                            let mut mempool_write = self.mempool.write().await;
                            let storage_read = self.storage.read().await;
                            for tx in reverted_txs {
                                let from = tx.recover_signer().unwrap_or_default();
                                let current_nonce = storage_read.transaction_count(from, alloy_eips::BlockId::Number(alloy_eips::BlockNumberOrTag::Latest)).await.unwrap_or(0);
                                mempool_write.add_transaction(tx, current_nonce);
                            }
                        }
                        
                        // Now sync from ancestor_height + 1
                        self.sync_range(&best_peer_url, ancestor_height + 1, best_height).await?;
                        return Ok(());
                    }
                    Err(e) => {
                        error!("[Sync] Failed to fetch hash from peer for divergence check: {}", e);
                    }
                    _ => {} // No divergence or peer missing block
                }
            }

            if best_height > local_height {
                debug!("[Sync] Found better peer with height {} (local: {})", best_height, local_height);
                self.sync_range(&best_peer_url, local_height + 1, best_height).await?;
            }
        } else {
            debug!("[Sync] No peers found with height greater than local {}", local_height);
        }

        Ok(())
    }

    async fn sync_range(&self, peer_url: &str, start: u64, end: u64) -> anyhow::Result<()> {
        debug!("[Sync] Starting sync range: {} to {}", start, end);
        self.is_syncing.store(true, Ordering::SeqCst);
        self.highest_block.store(end, Ordering::SeqCst);
        
        for next_block_num in start..=end {
            self.current_block.store(next_block_num, Ordering::SeqCst);
            debug!("[Sync] Fetching block {}", next_block_num);
            
            let block = match self.downloader.download_block(peer_url, next_block_num).await {
                Ok(block) => block,
                Err(e) => {
                    error!("[Sync] Failed to download block {}: {}", next_block_num, e);
                    self.is_syncing.store(false, Ordering::SeqCst);
                    return Err(e);
                }
            };

            // Check if parent exists in storage
            let parent_hash = block.header.parent_hash;
            let parent_exists = {
                let storage = self.storage.read().await;
                storage.get_block_by_hash(parent_hash).is_some()
            };

            if !parent_exists && next_block_num > 0 {
                debug!("[Sync] Parent block {:?} for {} missing, fetching ancestors", parent_hash, next_block_num);
                self.fetch_ancestors(peer_url, parent_hash).await?;
            }

            if let Err(e) = self.processor.process_block(block).await {
                error!("[Sync] Failed to process block {}: {}", next_block_num, e);
                self.is_syncing.store(false, Ordering::SeqCst);
                return Err(anyhow::anyhow!("Block processing failed"));
            }
        }
        
        self.is_syncing.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn fetch_ancestors(&self, peer_url: &str, hash: alloy_primitives::B256) -> anyhow::Result<()> {
        let mut current_hash = hash;
        let mut to_process = Vec::new();

        while current_hash != alloy_primitives::B256::ZERO {
            // Check if we already have it
            {
                let storage = self.storage.read().await;
                if storage.get_block_by_hash(current_hash).is_some() {
                    break;
                }
            }

            debug!("[Sync] Fetching ancestor block {:?}", current_hash);
            let block = self.downloader.download_block_by_hash(peer_url, current_hash).await?;
            let parent_hash = block.header.parent_hash;
            to_process.push(block);
            current_hash = parent_hash;

            if to_process.len() > 100 {
                debug!("[Sync] Ancestor chain too long, processing current batch");
                break;
            }
        }

        // Process in reverse order (oldest to newest)
        for block in to_process.into_iter().rev() {
            let block_num = block.header.number;
            if let Err(e) = self.processor.process_block(block).await {
                error!("[Sync] Failed to process ancestor block {}: {}", block_num, e);
                return Err(anyhow::anyhow!("Ancestor block processing failed"));
            }
        }

        Ok(())
    }
}
