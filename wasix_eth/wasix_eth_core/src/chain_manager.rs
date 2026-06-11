use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::{BlockProvider, ChangeSetProvider, HeaderProvider};
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::{AccountWriter, BlockWriter, ChangeSetWriter, HeaderWriter, StateWriter, StorageWriter, TransactionWriter};
use wasix_eth_types::{async_trait, Block, BlockId, SyncStatus, Transaction, TrieAccount, B256, U256};
use wasix_eth_utils::{debug, info, warn};
use alloy_rlp::Decodable;
use std::collections::{HashMap, HashSet};

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

    /// Marks a block as invalid and stores its parent hash.
    async fn add_invalid_block(&self, hash: B256, parent_hash: B256);

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


pub struct NoopChainManager;

#[async_trait]
impl ChainManager for NoopChainManager {
    async fn sync_status(&self) -> SyncStatus {
        SyncStatus::None
    }
    async fn set_sync_status(&self, _status: SyncStatus) {}
    async fn head_block(&self) -> (B256, u64) {
        (B256::ZERO, 0)
    }
    async fn set_head_block(&self, _hash: B256, _number: u64) {}
    async fn has_block(&self, _hash: B256) -> bool {
        false
    }
    async fn trigger_sync(&self) -> wasix_eth_types::Result<()> {
        Ok(())
    }
    async fn revert_to_height(&self, _height: u64) -> wasix_eth_types::Result<()> {
        Ok(())
    }
    async fn set_sync_trigger(&self, _trigger: Box<dyn Fn() + Send + Sync>) {}
    async fn is_invalid(&self, _hash: B256) -> bool { false }
    async fn add_invalid_block(&self, _hash: B256, _parent_hash: B256) {}
    async fn remove_invalid_block(&self, _hash: B256) {}
    async fn get_latest_valid_ancestor(&self, _hash: B256) -> Option<B256> { None }
    async fn add_sync_target(&self, _hash: B256, _peer_id: Option<String>) {}
    async fn pop_sync_target(&self) -> Option<(B256, Option<String>)> { None }
    async fn total_difficulty(&self) -> U256 { U256::ZERO }

    async fn determine_payload_status(&self, _head_block_hash: B256) -> wasix_eth_types::PayloadStatus {
        wasix_eth_types::PayloadStatus {
            status: wasix_eth_types::PayloadStatusEnum::Syncing,
            latest_valid_hash: None,
        }
    }

    async fn is_ancestor(&self, _head: B256, _target: B256) -> bool {
        false
    }

    async fn resolve_reorg(&self, _old_head: B256, _new_head: B256) -> wasix_eth_types::Result<ReorgContext> {
        Ok(ReorgContext {
            common_ancestor_hash: B256::ZERO,
            new_canonical_blocks: Vec::new(),
            is_reorg: false,
        })
    }

    async fn mark_branch_canonical(&self, _blocks: &Vec<Block<Transaction>>) -> wasix_eth_types::Result<()> {
        Ok(())
    }
}

pub struct ChainManagerImpl {
    read_storage: DatabaseReadProvider,
    write_storage: DatabaseWriteProvider,
    sync_status: tokio::sync::RwLock<SyncStatus>,
    head_block: tokio::sync::RwLock<(B256, u64)>,
    sync_trigger: tokio::sync::RwLock<Option<Box<dyn Fn() + Send + Sync>>>,
    invalid_blocks: tokio::sync::RwLock<HashMap<B256, B256>>,
    sync_targets: tokio::sync::RwLock<Vec<(B256, Option<String>)>>,
}

impl ChainManagerImpl {
    pub fn new(read_storage: DatabaseReadProvider, write_storage: DatabaseWriteProvider) -> Self {
        let (head_hash, head_number) = {
            let head = read_storage.forkchoice("head").unwrap_or(None).unwrap_or_default();
            let number = if head != B256::ZERO {
                read_storage.block_number(head).unwrap_or(None).unwrap_or_else(|| {
                    read_storage.latest_block_number().unwrap_or(None).unwrap_or_default()
                })
            } else {
                read_storage.latest_block_number().unwrap_or(None).unwrap_or_default()
            };
            (head, number)
        };

        Self {
            read_storage,
            write_storage: write_storage,
            sync_status: tokio::sync::RwLock::new(SyncStatus::None),
            head_block: tokio::sync::RwLock::new((head_hash, head_number)),
            sync_trigger: tokio::sync::RwLock::new(None),
            invalid_blocks: tokio::sync::RwLock::new(HashMap::new()),
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
        *self.head_block.read().await
    }

    async fn set_head_block(&self, hash: B256, number: u64) {
        let mut current = self.head_block.write().await;
        *current = (hash, number);
    }

    async fn has_block(&self, hash: B256) -> bool {
        // Check if header exists in storage
        matches!(self.read_storage.header(BlockId::Hash(hash.into())), Ok(Some(_)))
    }

    async fn revert_to_height(&self, height: u64) -> wasix_eth_types::Result<()> {
        let (_head_hash, head_number) = self.head_block().await;
        if height >= head_number {
            return Ok(());
        }

        info!("Reverting chain from {} to {}", head_number, height);

        // Revert state changes from head_number down to height + 1
        for h in (height + 1..=head_number).rev() {
            if let Some(account_changes) = self.read_storage.account_change_set(h)? {
                for (address, old_state) in account_changes {
                    if let Some(state) = old_state {
                        self.write_storage.update_plain_state(address, state.clone())?;

                        // Also update the Accounts table for consistency
                        let mut buf = &state[..];
                        let trie_acc = TrieAccount::decode(&mut buf)
                            .map_err(|e| anyhow::anyhow!("Failed to decode TrieAccount for {}: {}", address, e))?;
                        self.write_storage.update_account(address, trie_acc)?;
                    } else {
                        // Account was created in this block, so it didn't exist before.
                        // We must remove it from the state.
                        self.write_storage.remove_plain_state(address)?;
                        self.write_storage.remove_account(address)?;
                    }
                }
            }

            if let Some(storage_changes) = self.read_storage.storage_change_set(h)? {
                for (address, slot, old_value) in storage_changes {
                    self.write_storage.update_storage(address, slot, old_value)?;
                }
            }

            // Clean up the change sets themselves
            self.write_storage.remove_change_set(h)?;

            // Clean up HeaderNumbers mapping as well to prevent stale entries
            // although they might be correct for that hash, removing them ensures
            // a cleaner state during reorg transitions.
            if let Ok(Some(hash)) = self.read_storage.block_hash(h) {
                // There is no explicit remove_header_number, but we can potentially
                // just let mark_branch_canonical overwrite it.
                // For now, removing the canonical mapping is the most important part.
                let _ = hash; // avoid unused warning
            }

            // Remove from canonical heads
            self.write_storage.remove_canonical(h)?;
        }

        // Update canonical head
        let new_head_hash = self.read_storage.block_hash(height)?
            .ok_or_else(|| anyhow::anyhow!("Block hash not found for height {}", height))?;

        self.write_storage.set_canonical(height, new_head_hash)?;

        // Update cache
        self.set_head_block(new_head_hash, height).await;

        // Clear tracking maps to avoid polluting subsequent forward execution
        self.write_storage.clear_tracking();

        Ok(())
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
        self.invalid_blocks.read().await.contains_key(&hash)
    }

    async fn add_invalid_block(&self, hash: B256, parent_hash: B256) {
        let mut invalid_blocks = self.invalid_blocks.write().await;
        invalid_blocks.insert(hash, parent_hash);
    }

    async fn remove_invalid_block(&self, hash: B256) {
        let mut invalid_blocks = self.invalid_blocks.write().await;
        invalid_blocks.remove(&hash);
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
        let invalid_blocks = self.invalid_blocks.read().await;
        let mut current = hash;

        // If the starting hash is not in invalid_blocks, check if it's valid itself.
        if !invalid_blocks.contains_key(&current) {
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
            // It's unknown, so we can't say it's valid. Continue walk back if possible?
            // Actually, if it's unknown and not invalid, we might want to return None per spec
            // unless we find a valid ancestor.
        }

        // Walk back through invalid blocks
        // Add a safety limit and cycle detection
        let mut visited = HashSet::new();

        while invalid_blocks.contains_key(&current) {
            if !visited.insert(current) {
                // Cycle detected!
                warn!("Cycle detected in invalid_blocks at hash {:?}", current);
                return None;
            }
            if let Some(parent) = invalid_blocks.get(&current) {
                current = *parent;
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

    async fn resolve_reorg(&self, old_head: B256, new_head: B256) -> wasix_eth_types::Result<ReorgContext> {
        let mut new_canonical_blocks = Vec::new();
        let mut current_hash = new_head;
        let mut common_ancestor_hash = B256::ZERO;
        let mut common_ancestor_found = false;

        debug!("[ChainManager] Resolving reorg from {:?} to {:?}", old_head, new_head);

        while current_hash != B256::ZERO {
            if current_hash == old_head {
                debug!("[ChainManager] Found old head {:?} as common ancestor", current_hash);
                common_ancestor_hash = old_head;
                common_ancestor_found = true;
                break;
            }

            // Check if this block is already marked canonical
            if let Ok(Some(header)) = self.read_storage.header(BlockId::Hash(current_hash.into())) {
                if let Ok(Some(canonical_hash)) = self.read_storage.block_hash(header.number) {
                    if canonical_hash == current_hash {
                        debug!("[ChainManager] Found canonical block #{} hash {:?} as common ancestor", header.number, current_hash);
                        common_ancestor_hash = current_hash;
                        common_ancestor_found = true;
                        break;
                    }
                }

                if let Ok(Some(block)) = self.read_storage.block_by_hash(current_hash) {
                    new_canonical_blocks.push(block);
                } else if let Some((block, _, _)) = self.read_storage.get_payload_by_block_hash(current_hash) {
                    new_canonical_blocks.push(block.clone());
                } else {
                    return Err(anyhow::anyhow!("Block data missing for hash {:?} during reorg walk-back", current_hash).into());
                }

                current_hash = header.parent_hash;
            } else if let Some((block, _, _)) = self.read_storage.get_payload_by_block_hash(current_hash) {
                new_canonical_blocks.push(block.clone());
                current_hash = block.header.parent_hash;
            } else {
                debug!("[ChainManager] Walk-back reached unknown block {:?}", current_hash);
                break;
            }
        }

        let is_reorg = common_ancestor_found && old_head != common_ancestor_hash;
        if is_reorg {
             info!("[ChainManager] Reorg detected! Common ancestor: {:?} at height {}", 
                common_ancestor_hash, 
                self.read_storage.header(BlockId::Hash(common_ancestor_hash.into())).ok().flatten().map(|h| h.number).unwrap_or(0)
             );
        }

        Ok(ReorgContext {
            common_ancestor_hash,
            new_canonical_blocks,
            is_reorg,
        })
    }

    async fn mark_branch_canonical(&self, blocks: &Vec<Block<Transaction>>) -> wasix_eth_types::Result<()> {
        for block in blocks.iter().rev() {
            let hash = block.header.hash_slow();
            let number = block.header.number;

            info!("[ChainManager] Marking block {} (hash {}) as canonical", number, hash);

            // 1. Mark as canonical (Number -> Hash)
            self.write_storage.set_canonical(number, hash)?;

            // 2. Ensure Hash -> Number mapping is correct
            self.write_storage.insert_header_number(hash, number)?;
            self.write_storage.insert_block_hash(hash, number)?;

            // 3. Ensure TD is correct
            if let Ok(parent_td) = self.read_storage.header_td(block.header.parent_hash) {
                if let Some(ptd) = parent_td {
                    let expected_td = ptd + block.header.difficulty;
                    let current_td = self.read_storage.header_td(hash).ok().flatten();
                    if current_td != Some(expected_td) {
                        self.write_storage.insert_header_td(hash, expected_td)?;
                    }
                }
            }

            // 4. Update transaction lookups
            for (i, tx) in block.body.transactions.iter().enumerate() {
                let tx_hash = *tx.hash();
                self.write_storage.insert_transaction_lookup(tx_hash, hash, i as u64)?;
            }
        }
        Ok(())
    }
}
