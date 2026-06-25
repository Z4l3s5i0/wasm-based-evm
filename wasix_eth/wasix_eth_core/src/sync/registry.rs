use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};
use wasix_eth_types::B256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Idle,
    Syncing,
    Stalled,
}

pub struct SyncRegistry {
    targets: RwLock<VecDeque<(B256, Option<String>)>>,
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

    pub async fn add_target(&self, hash: B256, peer_id: Option<String>, chain_manager: Arc<dyn crate::ChainManager>) {
        if chain_manager.has_block(hash).await {
            return; // Already have it
        }
        let mut targets = self.targets.write().await;
        // Check if already present
        if !targets.iter().any(|(h, _)| *h == hash) {
            targets.push_back((hash, peer_id));
            self.notify.notify_one();
        }
    }

    pub async fn pop_target(&self) -> Option<(B256, Option<String>)> {
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
