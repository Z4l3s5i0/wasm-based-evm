use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::{BlockProvider, HeaderProvider};
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_types::{async_trait, Block, BlockId, SyncStatus, Transaction, B256, U256};
use wasix_eth_utils::warn;
use crate::engine::canonicality_tracker::CanonicalState;
pub(crate) use crate::engine::sidechain_tracker::{BlockTree, InvalidationReason};
use crate::engine::reorg_manager::ReorgHandler;
use std::sync::Arc;
use std::collections::HashSet;


#[async_trait]
pub trait ChainManager: Send + Sync {
    /// Returns the current synchronization status.
    async fn sync_status(&self) -> SyncStatus;

    /// Sets the current synchronization status.
    async fn set_sync_status(&self, status: SyncStatus);

    /// Returns the current head block (hash and number).
    async fn head_block(&self) -> (B256, u64);

    /// Sets the current head block.
    async fn set_head_block(&self, hash: B256, number: u64);

    /// Checks if the node has a block with the given hash.
    async fn has_block(&self, hash: B256) -> bool;

    /// Triggers a synchronization process.
    async fn trigger_sync(&self) -> wasix_eth_types::Result<()>;

    /// Reverts the chain to a specific height.
    async fn revert_to_height(&self, height: u64) -> wasix_eth_types::Result<()>;
    
    /// Sets a callback or channel to trigger sync in the SyncController.
    async fn set_sync_trigger(&self, trigger: Box<dyn Fn() + Send + Sync>);

    /// Checks if the given block hash is known to be invalid.
    async fn is_invalid(&self, hash: B256) -> bool;

    /// Gets the invalidation reason for a block.
    async fn get_invalidation_reason(&self, hash: B256) -> Option<InvalidationReason>;

    /// Marks a block as invalid and stores its parent hash.
    async fn add_invalid_block(&self, hash: B256, parent_hash: B256, reason: InvalidationReason);

    /// Removes a block from the invalid blocks list.
    async fn remove_invalid_block(&self, hash: B256);

    /// Returns the latest known valid ancestor of an invalid block.
    async fn get_latest_valid_ancestor(&self, hash: B256) -> Option<B256>;

    /// Adds a hash that we need to sync, optionally with a peer affinity.
    async fn add_sync_target(&self, hash: B256, peer_id: Option<String>);

    /// Pops a sync target from the list of pending targets (returns hash and optional peer affinity).
    async fn pop_sync_target(&self) -> Option<(B256, Option<String>)>;

    /// Retrieves the total difficulty of the head block.
    async fn total_difficulty(&self) -> U256;

    async fn determine_payload_status(&self, head_block_hash: B256) -> wasix_eth_types::PayloadStatus;
    async fn is_ancestor(&self, head: B256, target: B256) -> bool;

    /// Resolves a reorg by comparing the old and new heads.
    async fn resolve_reorg(&self, old_head: B256, new_head: B256) -> wasix_eth_types::Result<ReorgContext>;

    /// Marks a branch as canonical in storage.
    async fn mark_branch_canonical(&self, blocks: &Vec<Block<Transaction>>) -> wasix_eth_types::Result<()>;
}

#[derive(Debug, Clone)]
pub struct ReorgContext {
    pub common_ancestor_hash: B256,
    pub new_canonical_blocks: Vec<Block<Transaction>>,
    pub is_reorg: bool,
}


pub struct ChainManagerImpl {
    read_storage: DatabaseReadProvider,
    _write_storage: DatabaseWriteProvider,
    sync_status: tokio::sync::RwLock<SyncStatus>,
    pub canonical: Arc<CanonicalState>,
    pub block_tree: Arc<BlockTree>,
    pub reorg_handler: Arc<ReorgHandler>,
    sync_trigger: tokio::sync::RwLock<Option<Box<dyn Fn() + Send + Sync>>>,
    sync_targets: tokio::sync::RwLock<Vec<(B256, Option<String>)>>,
}

impl ChainManagerImpl {
    pub fn new(
        read_storage: DatabaseReadProvider,
        write_storage: DatabaseWriteProvider,
        canonical: Arc<CanonicalState>,
        block_tree: Arc<BlockTree>,
        reorg_handler: Arc<ReorgHandler>,
    ) -> Self {
        Self {
            read_storage,
            _write_storage: write_storage,
            sync_status: tokio::sync::RwLock::new(SyncStatus::None),
            canonical,
            block_tree,
            reorg_handler,
            sync_trigger: tokio::sync::RwLock::new(None),
            sync_targets: tokio::sync::RwLock::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ChainManager for ChainManagerImpl {
    async fn sync_status(&self) -> SyncStatus {
        self.sync_status.read().await.clone()
    }

    async fn set_sync_status(&self, status: SyncStatus) {
        let mut current = self.sync_status.write().await;
        *current = status;
    }

    async fn head_block(&self) -> (B256, u64) {
        self.canonical.get_head().await
    }

    async fn set_head_block(&self, hash: B256, number: u64) {
        let _ = self.canonical.update_head(hash, number).await;
    }

    async fn has_block(&self, hash: B256) -> bool {
        // Check if header exists in storage
        matches!(self.read_storage.header(BlockId::Hash(hash.into())), Ok(Some(_)))
    }


    async fn trigger_sync(&self) -> wasix_eth_types::Result<()> {
        if let Some(trigger) = self.sync_trigger.read().await.as_ref() {
            trigger();
        }
        Ok(())
    }

    async fn set_sync_trigger(&self, trigger: Box<dyn Fn() + Send + Sync>) {
        let mut current = self.sync_trigger.write().await;
        *current = Some(trigger);
    }

    async fn is_invalid(&self, hash: B256) -> bool {
        self.block_tree.get_invalidation_reason(hash).await.is_some()
    }

    async fn get_invalidation_reason(&self, hash: B256) -> Option<InvalidationReason> {
        self.block_tree.get_invalidation_reason(hash).await
    }

    async fn add_invalid_block(&self, hash: B256, parent_hash: B256, reason: InvalidationReason) {
        self.block_tree.mark_invalid(hash, parent_hash, reason).await;
    }

    async fn remove_invalid_block(&self, hash: B256) {
        self.block_tree.remove_invalid(hash).await;
    }

    async fn add_sync_target(&self, hash: B256, peer_id: Option<String>) {
        let mut targets = self.sync_targets.write().await;
        if !targets.iter().any(|(h, _)| *h == hash) {
            targets.push((hash, peer_id));
        }
    }

    async fn pop_sync_target(&self) -> Option<(B256, Option<String>)> {
        let mut targets = self.sync_targets.write().await;
        targets.pop()
    }

    async fn get_latest_valid_ancestor(&self, hash: B256) -> Option<B256> {
        let mut current = hash;

        // If the starting hash is not in invalid_blocks, check if it's valid itself.
        if self.get_invalidation_reason(current).await.is_none() {
            if self.has_block(current).await {
                return Some(current);
            }
            // Check if it's genesis
            if let Ok(Some(genesis_hash)) = self.read_storage.block_hash(0) {
                if current == genesis_hash {
                    return Some(current);
                }
            }
            // If current is ZERO, it means we reached the "parent" of genesis or a PoW block was recorded as parent.
            if current == B256::ZERO {
                return Some(B256::ZERO);
            }
        }

        // Walk back through invalid blocks
        // Add a safety limit and cycle detection
        let mut visited = HashSet::new();

        while let Some(_reason) = self.get_invalidation_reason(current).await {
            if !visited.insert(current) {
                // Cycle detected!
                warn!("Cycle detected in invalid_blocks at hash {:?}", current);
                return None;
            }
            if let Some(parent) = self.block_tree.get_invalid_parent(current).await {
                current = parent;
            } else {
                break;
            }

            if visited.len() >= 1024 {
                warn!("Ancestry walk limit reached for hash {:?}", hash);
                return None;
            }
        }

        // Now current is the first non-invalid block we found.
        // It might be valid or just unknown.
        if self.has_block(current).await {
            Some(current)
        } else {
            // Check if it's genesis
            if let Ok(Some(genesis_hash)) = self.read_storage.block_hash(0) {
                if current == genesis_hash {
                    return Some(current);
                }
            }

            // The spec says:
            // "0x0000000000000000000000000000000000000000000000000000000000000000 if the above conditions are satisfied by a PoW block."
            // "null if client software cannot determine the ancestor of the invalid payload satisfying the above conditions."

            // If current is ZERO, it means we reached the "parent" of genesis or a PoW block was recorded as parent.
            if current == B256::ZERO {
                return Some(B256::ZERO);
            }

            // We can't determine if 'current' is valid because it's unknown.
            None
        }
    }

    async fn total_difficulty(&self) -> U256 {
        let (head_hash, _) = self.head_block().await;
        self.read_storage.header_td(head_hash).ok().flatten().unwrap_or(U256::ZERO)
    }

    async fn determine_payload_status(&self, head_block_hash: B256) -> wasix_eth_types::PayloadStatus {
        if self.is_invalid(head_block_hash).await {
            let latest_valid = self.get_latest_valid_ancestor(head_block_hash).await;
            return wasix_eth_types::PayloadStatus {
                status: wasix_eth_types::PayloadStatusEnum::Invalid { validation_error: "Block is known to be invalid".to_string() },
                latest_valid_hash: latest_valid,
            };
        }

        if let Ok(Some(_)) = self.read_storage.header(wasix_eth_types::BlockId::Hash(head_block_hash.into())) {
            return wasix_eth_types::PayloadStatus {
                status: wasix_eth_types::PayloadStatusEnum::Valid,
                latest_valid_hash: Some(head_block_hash),
            };
        }

        // Check if it's in the payloads map (processed but not yet canonical)
        if let Some(_) = self.read_storage.get_payload_by_block_hash(head_block_hash) {
            return wasix_eth_types::PayloadStatus {
                status: wasix_eth_types::PayloadStatusEnum::Valid,
                latest_valid_hash: Some(head_block_hash),
            };
        }

        wasix_eth_types::PayloadStatus {
            status: wasix_eth_types::PayloadStatusEnum::Syncing,
            latest_valid_hash: None,
        }
    }

    async fn is_ancestor(&self, head: B256, target: B256) -> bool {
        if head == target {
            return true;
        }
        if target == B256::ZERO {
            return true;
        }
        let mut current = head;
        // Search up to 2048 blocks back
        for _ in 0..2048 {
            let parent_hash = if let Ok(Some(header)) = self.read_storage.header(wasix_eth_types::BlockId::Hash(current.into())) {
                Some(header.parent_hash)
            } else if let Some((payload, _, _)) = self.read_storage.get_payload_by_block_hash(current) {
                Some(payload.header.parent_hash)
            } else {
                let genesis_hash = self.read_storage.block_hash(0).unwrap_or(None);
                if Some(current) == genesis_hash {
                    Some(B256::ZERO)
                } else {
                    None
                }
            };

            if let Some(parent) = parent_hash {
                if parent == target {
                    return true;
                }
                if parent == B256::ZERO {
                    break;
                }
                current = parent;
            } else {
                break;
            }
        }
        false
    }

    async fn revert_to_height(&self, height: u64) -> wasix_eth_types::Result<()> {
        self.reorg_handler.revert_to_height(height).await
    }

    async fn resolve_reorg(&self, old_head: B256, new_head: B256) -> wasix_eth_types::Result<ReorgContext> {
        self.reorg_handler.resolve_reorg(old_head, new_head).await
    }

    async fn mark_branch_canonical(&self, blocks: &Vec<Block<Transaction>>) -> wasix_eth_types::Result<()> {
        self.reorg_handler.mark_branch_canonical(blocks).await
    }
}
