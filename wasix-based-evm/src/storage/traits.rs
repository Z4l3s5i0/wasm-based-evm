use alloy_primitives::{Address, B256, U256, Bytes};
use alloy_consensus::{Header, Block, TxEnvelope as Transaction, ReceiptWithBloom as Receipt};
use alloy_eips::BlockId;
use anyhow::Result;

pub trait StateProvider: Send + Sync {
    fn account(&self, address: Address, block_id: BlockId) -> Result<Option<crate::storage::genesis::GenesisAccount>>;
    fn storage(&self, address: Address, slot: B256, block_id: BlockId) -> Result<Option<U256>>;
    fn code(&self, address: Address, block_id: BlockId) -> Result<Option<Bytes>>;
    fn balance(&self, address: Address, block_id: BlockId) -> Result<U256>;
    fn transaction_count(&self, address: Address, block_id: BlockId) -> Result<u64>;
}

pub trait BlockProvider: Send + Sync {
    fn header(&self, block_id: BlockId) -> Result<Option<Header>>;
    fn block(&self, block_id: BlockId) -> Result<Option<Block<Transaction>>>;
    fn block_hash(&self, number: u64) -> Result<Option<B256>>;
    fn latest_block_number(&self) -> Result<u64>;
}

pub trait TransactionProvider: Send + Sync {
    fn transaction(&self, hash: B256) -> Result<Option<Transaction>>;
    fn transaction_receipt(&self, hash: B256) -> Result<Option<Receipt>>;
    fn transaction_block_reference(&self, hash: B256) -> Result<Option<(u64, B256, usize)>>; // (number, hash, index)
}

pub trait LogProvider: Send + Sync {
    fn logs(&self, filter: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::Log>>;
}
