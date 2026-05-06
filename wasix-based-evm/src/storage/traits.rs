use alloy_primitives::{Address, B256, U256, Bytes};
use alloy_consensus::{Header, Block, ReceiptWithBloom as Receipt, TxEnvelope as Transaction};
use alloy_eips::BlockId;
use alloy_genesis::GenesisAccount;
use alloy_rpc_types::engine::PayloadId;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait StateProvider: Send + Sync {
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
}

pub trait ChainProvider: Send + Sync {
    fn add_block(&mut self, block: Block<Transaction>);
    fn revert_to_height(&mut self, height: u64) -> Vec<Transaction>;
    fn add_transaction(&mut self, tx: Transaction);
    fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt);
    fn calculate_state_root(&self) -> B256;
    fn add_payload(&mut self, payload_id: PayloadId, block: Block<Transaction>, receipts: Vec<Receipt>);
    fn get_payload(&self, payload_id: &PayloadId) -> Option<&(Block<Transaction>, Vec<Receipt>)>;
    fn remove_payload(&mut self, payload_id: &PayloadId) -> Option<(Block<Transaction>, Vec<Receipt>)>;
    fn update_forkchoice(&mut self, head: B256, safe: Option<B256>, finalized: Option<B256>);
}


pub trait FullProvider: StateProvider + ChainProvider {}

impl<T> FullProvider for T where T: StateProvider + ChainProvider {}