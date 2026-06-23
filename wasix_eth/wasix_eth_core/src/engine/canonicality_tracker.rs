use alloy_primitives::B256;
use tokio::sync::RwLock;
use wasix_eth_storage::{BlockProvider, BlockWriter};
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;

pub struct CanonicalState {
    _read_storage: DatabaseReadProvider,
    write_storage: DatabaseWriteProvider,
    /// (Hash, Number) of the current canonical head
    head: RwLock<(B256, u64)>,
    safe_hash: RwLock<B256>,
    finalized_hash: RwLock<B256>,
}

impl CanonicalState {
    pub fn new(read_storage: DatabaseReadProvider, write_storage: DatabaseWriteProvider) -> Self {
        let head_hash = read_storage.forkchoice("head").unwrap_or(None).unwrap_or_default();
        let head_number = if head_hash != B256::ZERO {
            read_storage.block_number(head_hash).unwrap_or(None).unwrap_or_default()
        } else {
            0
        };
        let safe_hash = read_storage.forkchoice("safe").unwrap_or(None).unwrap_or_default();
        let finalized_hash = read_storage.forkchoice("finalized").unwrap_or(None).unwrap_or_default();

        Self {
            _read_storage: read_storage,
            write_storage,
            head: RwLock::new((head_hash, head_number)),
            safe_hash: RwLock::new(safe_hash),
            finalized_hash: RwLock::new(finalized_hash),
        }
    }

    pub async fn get_head(&self) -> (B256, u64) {
        *self.head.read().await
    }

    pub async fn get_safe(&self) -> B256 {
        *self.safe_hash.read().await
    }

    pub async fn get_finalized(&self) -> B256 {
        *self.finalized_hash.read().await
    }

    pub async fn update_head(&self, hash: B256, number: u64) -> anyhow::Result<()> {
        let mut head = self.head.write().await;
        *head = (hash, number);
        let safe = *self.safe_hash.read().await;
        let finalized = *self.finalized_hash.read().await;
        self.write_storage.update_forkchoice(hash, Some(safe), Some(finalized))?;
        Ok(())
    }

    pub async fn update_safe(&self, hash: B256) -> anyhow::Result<()> {
        let mut safe = self.safe_hash.write().await;
        *safe = hash;
        let (head, _) = *self.head.read().await;
        let finalized = *self.finalized_hash.read().await;
        self.write_storage.update_forkchoice(head, Some(hash), Some(finalized))?;
        Ok(())
    }

    pub async fn update_finalized(&self, hash: B256) -> anyhow::Result<()> {
        let mut finalized = self.finalized_hash.write().await;
        *finalized = hash;
        let (head, _) = *self.head.read().await;
        let safe = *self.safe_hash.read().await;
        self.write_storage.update_forkchoice(head, Some(safe), Some(hash))?;
        Ok(())
    }
}