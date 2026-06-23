use alloy_primitives::B256;
use wasix_eth_storage::{AccountWriter, BlockProvider, BlockWriter, ChangeSetProvider, ChangeSetWriter, HeaderProvider, HeaderWriter, StateWriter, StorageWriter, TransactionWriter};
use wasix_eth_storage::codecs::RedbRlp;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_types::{Block, BlockId, Transaction, TrieAccount};
use wasix_eth_utils::{debug, info};
use crate::chain_manager::ReorgContext;

use crate::engine::canonicality_tracker::CanonicalState;
use std::sync::Arc;

pub struct ReorgHandler{
    read_storage: DatabaseReadProvider,
    write_storage: DatabaseWriteProvider,
    canonical: Arc<CanonicalState>,
}

impl ReorgHandler {
    pub fn new(read_provider: DatabaseReadProvider, write_provider: DatabaseWriteProvider, canonical: Arc<CanonicalState>) -> Self {
        Self {
            read_storage: read_provider,
            write_storage: write_provider,
            canonical,
        }
    }

    pub async fn resolve_reorg(&self, old_head: B256, new_head: B256) -> wasix_eth_types::Result<ReorgContext> {
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

            // Check if this block is already marked canonical in O(1) if possible
            if let Ok(Some(number)) = self.read_storage.block_number(current_hash) {
                if let Ok(Some(canonical_hash)) = self.read_storage.block_hash(number) {
                    if canonical_hash == current_hash {
                        debug!("[ChainManager] Found canonical block #{} hash {:?} as common ancestor", number, current_hash);
                        common_ancestor_hash = current_hash;
                        common_ancestor_found = true;
                        break;
                    }
                }
            }

            // If not canonical, we need to walk back
            if let Ok(Some(header)) = self.read_storage.header(BlockId::Hash(current_hash.into())) {
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

    pub async fn mark_branch_canonical(&self, blocks: &Vec<Block<Transaction>>) -> wasix_eth_types::Result<()> {
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


    pub async fn revert_to_height(&self, height: u64) -> wasix_eth_types::Result<()> {
        let (_, head_number) = self.canonical.get_head().await;
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

        self.canonical.update_head(new_head_hash, height).await?;

        // Clear tracking maps to avoid polluting subsequent forward execution
        self.write_storage.clear_tracking();

        Ok(())
    }

}