use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use wasix_eth_types::Result;
use wasix_eth_types::sync::SyncProvider;
use wasix_eth_utils::{debug, error, info};
use crate::sync::registry::SyncRegistry;

/// The SyncOrchestrator connects the SyncRegistry to the SyncProvider.
/// It runs as a background task, listening for new sync targets and triggering the provider.
pub struct SyncOrchestrator {
    pub registry: Arc<SyncRegistry>,
    sync_provider: Arc<dyn SyncProvider>,
}

impl SyncOrchestrator {
    pub fn new(registry: Arc<SyncRegistry>, sync_provider: Arc<dyn SyncProvider>) -> Self {
        Self {
            registry,
            sync_provider,
        }
    }

    /// Start the orchestrator background loop.
    pub async fn run(self: Arc<Self>) {
        info!("[SyncOrchestrator] Starting background loop");
        let notify = self.registry.subscribe();
        
        loop {
            // Wait for a notification that new targets are available
            notify.notified().await;
            
            if let Err(e) = self.evaluate_and_trigger().await {
                error!("[SyncOrchestrator] Error triggering sync: {}", e);
            }

            sleep(Duration::from_secs(1)).await;
        }
    }

    /// Evaluate the current state and trigger sync if necessary.
    pub async fn evaluate_and_trigger(&self) -> Result<()> {
        if !self.registry.has_targets().await {
            return Ok(());
        }

        debug!("[SyncOrchestrator] Targets available, triggering sync");
        
        if let Err(e) = self.sync_provider.trigger_sync().await {
            error!("[SyncOrchestrator] Failed to trigger sync: {}", e);
            return Err(e);
        }
        
        Ok(())
    }
}
