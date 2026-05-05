use std::sync::Arc;
use alloy_primitives::{Address, B256, U256, Bytes};
use alloy_consensus::{Header, Block, ReceiptWithBloom as Receipt, TxEnvelope as Transaction};
use alloy_eips::BlockId;
use alloy_genesis::GenesisAccount;
use alloy_rpc_types::engine::PayloadId;
use alloy_rpc_types::Withdrawal;
use evm::backend::{OverlayedChangeSet, InMemoryAccount};
use anyhow::Result;
use async_trait::async_trait;
use std::any::Any;

#[async_trait]
pub trait StateSnapshot: SyncStateProvider + StateProvider + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub trait SyncStateProvider: Send + Sync {
    fn set_block_environment(&mut self, number: u64, timestamp: u64, base_fee: U256);
    fn get_account(&self, address: Address) -> Option<InMemoryAccount>;
    fn apply_changeset(&mut self, changeset: &OverlayedChangeSet);
    fn backend(&self) -> &evm::backend::InMemoryBackend;
    fn set_account(&mut self, address: Address, account: InMemoryAccount);
    fn calculate_state_root(&self) -> B256;
    fn clone_box(&self) -> Box<dyn SyncStateProvider>;
}

impl Clone for Box<dyn SyncStateProvider> {
    fn clone(&self) -> Box<dyn SyncStateProvider> {
        self.clone_box()
    }
}

#[async_trait]
pub trait StateProvider: Send + Sync {
    fn writer(&self) -> Arc<dyn WriteProvider>;
    async fn account(&self, address: Address, block_id: BlockId) -> Result<Option<GenesisAccount>>;
    async fn storage(&self, address: Address, slot: B256, block_id: BlockId) -> Result<Option<U256>>;
    async fn code(&self, address: Address, block_id: BlockId) -> Result<Option<Bytes>>;
    async fn balance(&self, address: Address, block_id: BlockId) -> Result<U256>;
    async fn transaction_count(&self, address: Address, block_id: BlockId) -> Result<u64>;
    async fn accounts(&self) -> Result<Vec<Address>>;
    async fn header(&self, block_id: BlockId) -> Result<Option<Header>>;
    async fn block(&self, block_id: BlockId) -> Result<Option<Block<Transaction>>>;
    async fn block_hash(&self, number: u64) -> Result<Option<B256>>;
    async fn latest_block_number(&self) -> Result<u64>;
    async fn chain_id(&self) -> Result<u64>;
    async fn logs(&self, filter: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::eth::Log>>;
    async fn transaction(&self, hash: B256) -> Result<Option<Transaction>>;
    async fn transaction_receipt(&self, hash: B256) -> Result<Option<Receipt>>;
    async fn transaction_block_reference(&self, hash: B256) -> Result<Option<(u64, B256, usize)>>; // (number, hash, index)

    /// Returns a snapshot of the underlying storage for simulations
    async fn get_snapshot(&self) -> Result<Box<dyn StateSnapshot>>;
}


#[async_trait]
pub trait WriteProvider: Send + Sync {
    async fn add_block(&self, block: Block<Transaction>) -> Result<()>;
    async fn add_transaction(&self, tx: Transaction) -> Result<()>;
    async fn add_receipt(&self, tx_hash: B256, receipt: Receipt) -> Result<()>;
    async fn set_balance(&self, address: Address, balance: U256) -> Result<()>;
    async fn add_payload(&self, payload_id: PayloadId, block: Block<Transaction>, receipts: Vec<Receipt>) -> Result<()>;
    async fn remove_payload(&self, payload_id: &PayloadId) -> Result<Option<(Block<Transaction>, Vec<Receipt>)>>;
    async fn update_forkchoice(&self, head: B256, safe: Option<B256>, finalized: Option<B256>) -> Result<()>;
    async fn revert_to_height(&self, height: u64) -> Result<Vec<Transaction>>;
    async fn commit_block(&self, block: Block<Transaction>, receipts: Vec<Receipt>, changeset: OverlayedChangeSet, withdrawals: Vec<Withdrawal>) -> Result<()>;
    async fn apply_state_changeset(&self, changeset: OverlayedChangeSet) -> Result<()>;
}

pub trait Database: Send + Sync {
    fn state_store(&self) -> Arc<dyn StateProvider>;
    fn chain_store(&self) -> Arc<dyn StateProvider>;
}
