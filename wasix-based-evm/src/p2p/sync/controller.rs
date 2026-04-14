use std::sync::Arc;
use tokio::sync::RwLock;
use crate::storage::storage::InMemoryStorage;
use crate::p2p::peer_manager::PeerManager;
use crate::executor::Executor;
use crate::p2p::sync::downloader::Downloader;
use crate::p2p::sync::processor::BlockProcessor;
use crate::{info, error, debug};

pub struct SyncController {
    storage: Arc<RwLock<InMemoryStorage>>,
    downloader: Downloader,
    processor: BlockProcessor,
}

impl SyncController {
    pub fn new(storage: Arc<RwLock<InMemoryStorage>>, peer_manager: Arc<PeerManager>, executor: Arc<Executor>) -> Self {
        Self {
            storage: storage.clone(),
            downloader: Downloader::new(peer_manager),
            processor: BlockProcessor::new(storage, executor),
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

    async fn sync_step(&self) -> anyhow::Result<()> {
        let local_height = {
            let storage_read = self.storage.read().await;
            storage_read.get_latest_block_number()
        };

        if let Some((best_peer_url, best_height)) = self.downloader.get_best_peer(local_height).await {
            if best_height > local_height {
                info!("[Sync] Found better peer with height {} (local: {})", best_height, local_height);
                for next_block_num in (local_height + 1)..=best_height {
                    debug!("[Sync] Fetching block {}", next_block_num);
                    match self.downloader.download_block(&best_peer_url, next_block_num).await {
                        Ok(block) => {
                            if let Err(e) = self.processor.process_block(block).await {
                                error!("[Sync] Failed to process block {}: {}", next_block_num, e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("[Sync] Failed to download block {}: {}", next_block_num, e);
                            break;
                        }
                    }
                }
            }
        } else {
            debug!("[Sync] No peers found with height greater than local {}", local_height);
        }

        Ok(())
    }
}
