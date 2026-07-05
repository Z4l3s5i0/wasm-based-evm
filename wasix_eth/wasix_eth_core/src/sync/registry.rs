use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};
use wasix_eth_types::{ChainManager, B256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Idle,
    Syncing,
    Stalled,
}

#[derive(Debug, Clone)]
pub struct SyncTarget {
    pub hash: B256,
    pub peer_id: Option<String>,
    pub retries: u32,
}

pub struct SyncRegistry {
    targets: RwLock<VecDeque<SyncTarget>>,
    status: RwLock<SyncStatus>,
    notify: Arc<Notify>,
}

impl SyncRegistry {
    pub fn new() -> Self {
        Self {
            targets: RwLock::new(VecDeque::new()),
            status: RwLock::new(SyncStatus::Idle),
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn add_target(&self, hash: B256, peer_id: Option<String>, chain_manager: Arc<dyn ChainManager>) {
        self.add_target_with_retries(hash, peer_id, 0, chain_manager).await;
    }

    pub async fn add_target_with_retries(&self, hash: B256, peer_id: Option<String>, retries: u32, chain_manager: Arc<dyn ChainManager>) {
        if chain_manager.has_block(hash).await {
            return; // Already have it
        }
        let mut targets = self.targets.write().await;
        // Check if already present
        if !targets.iter().any(|t| t.hash == hash) {
            targets.push_back(SyncTarget { hash, peer_id, retries });
            self.notify.notify_one();
        }
    }

    pub async fn pop_target(&self) -> Option<SyncTarget> {
        let mut targets = self.targets.write().await;
        targets.pop_front()
    }

    pub async fn get_status(&self) -> SyncStatus {
        *self.status.read().await
    }

    pub async fn set_status(&self, status: SyncStatus) {
        let mut current_status = self.status.write().await;
        *current_status = status;
    }

    pub fn subscribe(&self) -> Arc<Notify> {
        self.notify.clone()
    }
    
    pub async fn has_targets(&self) -> bool {
        !self.targets.read().await.is_empty()
    }

    pub async fn clear_targets(&self) {
        let mut targets = self.targets.write().await;
        targets.clear();
    }

    pub async fn is_syncing(&self) -> bool {
        let status = self.status.read().await;
        *status == SyncStatus::Syncing
    }
}
