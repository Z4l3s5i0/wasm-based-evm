use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;
use alloy_primitives::{Address, B256, U256, Bytes, B64};
use alloy_consensus::{Block, Header, ReceiptWithBloom as Receipt, TxEnvelope as Transaction};
use alloy_rpc_types::engine::PayloadId;
use alloy_eips::BlockId;
use alloy_genesis::GenesisAccount;
use anyhow::Result;

use crate::storage::traits::{StateProvider, WriteProvider};
use crate::storage::storage::InMemoryStorage;
use crate::storage::mempool::Mempool;
use crate::evm::executor::Executor;

pub struct StorageProvider {
    inner: Arc<RwLock<InMemoryStorage>>,
    mempool: Arc<RwLock<Mempool>>,
    executor: Arc<Executor>,
    writer: StorageWriter,
}

#[derive(Clone)]
pub struct StorageWriter {
    inner: Arc<RwLock<InMemoryStorage>>,
}

impl StorageWriter {
    pub fn new(inner: Arc<RwLock<InMemoryStorage>>) -> Self {
        Self { inner }
    }
}

impl StorageProvider {
    pub fn new(
        storage: Arc<RwLock<InMemoryStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        executor: Arc<Executor>,
    ) -> Self {
        Self {
            inner: storage.clone(),
            mempool,
            executor,
            writer: StorageWriter::new(storage),
        }
    }

    pub fn writer(&self) -> &StorageWriter {
        &self.writer
    }

    async fn get_pending_state(&self) -> Result<InMemoryStorage> {
        let storage = self.inner.read().await.clone();
        let transactions = self.mempool.read().await.peek_transactions(100); // Take a reasonable amount for pending
        
        if transactions.is_empty() {
            return Ok(storage);
        }

        let mut pending_storage = storage;
        let latest_block = pending_storage.get_latest_block().cloned().unwrap();
        let pending_block = Block {
            header: Header {
                number: latest_block.header.number + 1,
                parent_hash: latest_block.header.hash_slow(),
                timestamp: latest_block.header.timestamp + 1,
                beneficiary: latest_block.header.beneficiary,
                gas_limit: latest_block.header.gas_limit,
                difficulty: latest_block.header.difficulty,
                mix_hash: latest_block.header.mix_hash,
                base_fee_per_gas: latest_block.header.base_fee_per_gas,
                ommers_hash: alloy_consensus::EMPTY_OMMER_ROOT_HASH,
                state_root: B256::ZERO,
                transactions_root: B256::ZERO,
                receipts_root: B256::ZERO,
                logs_bloom: Default::default(),
                gas_used: 0,
                extra_data: Bytes::new(),
                nonce: B64::ZERO,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                parent_beacon_block_root: None,
                requests_hash: None,
            },
            body: alloy_consensus::BlockBody {
                transactions: transactions.clone(),
                ..Default::default()
            },
        };

        // We don't want to fail if some txs in mempool are invalid, just apply what we can
        let _ = self.executor.execute_block(&mut pending_storage, transactions, pending_block);
        
        Ok(pending_storage)
    }
}

#[async_trait]
impl StateProvider for StorageProvider {
    fn writer(&self) -> Arc<dyn WriteProvider> {
        Arc::new(self.writer.clone())
    }

    async fn account(&self, address: Address, block_id: BlockId) -> Result<Option<GenesisAccount>> {
        if matches!(block_id, BlockId::Number(alloy_eips::BlockNumberOrTag::Pending)) {
            return self.get_pending_state().await?.account(address, block_id).await;
        }
        self.inner.read().await.account(address, block_id).await
    }

    async fn storage(&self, address: Address, slot: B256, block_id: BlockId) -> Result<Option<U256>> {
        if matches!(block_id, BlockId::Number(alloy_eips::BlockNumberOrTag::Pending)) {
            return self.get_pending_state().await?.storage(address, slot, block_id).await;
        }
        self.inner.read().await.storage(address, slot, block_id).await
    }

    async fn code(&self, address: Address, block_id: BlockId) -> Result<Option<Bytes>> {
        if matches!(block_id, BlockId::Number(alloy_eips::BlockNumberOrTag::Pending)) {
            return self.get_pending_state().await?.code(address, block_id).await;
        }
        self.inner.read().await.code(address, block_id).await
    }

    async fn balance(&self, address: Address, block_id: BlockId) -> Result<U256> {
        if matches!(block_id, BlockId::Number(alloy_eips::BlockNumberOrTag::Pending)) {
            return self.get_pending_state().await?.balance(address, block_id).await;
        }
        self.inner.read().await.balance(address, block_id).await
    }

    async fn transaction_count(&self, address: Address, block_id: BlockId) -> Result<u64> {
        if matches!(block_id, BlockId::Number(alloy_eips::BlockNumberOrTag::Pending)) {
            return self.get_pending_state().await?.transaction_count(address, block_id).await;
        }
        self.inner.read().await.transaction_count(address, block_id).await
    }

    async fn accounts(&self) -> Result<Vec<Address>> {
        self.inner.read().await.accounts().await
    }

    async fn header(&self, block_id: BlockId) -> Result<Option<Header>> {
        self.inner.read().await.header(block_id).await
    }

    async fn block(&self, block_id: BlockId) -> Result<Option<Block<Transaction>>> {
        self.inner.read().await.block(block_id).await
    }

    async fn block_hash(&self, number: u64) -> Result<Option<B256>> {
        self.inner.read().await.block_hash(number).await
    }

    async fn latest_block_number(&self) -> Result<u64> {
        self.inner.read().await.latest_block_number().await
    }

    async fn chain_id(&self) -> Result<u64> {
        self.inner.read().await.chain_id().await
    }

    async fn logs(&self, filter: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::eth::Log>> {
        self.inner.read().await.logs(filter).await
    }

    async fn transaction(&self, hash: B256) -> Result<Option<Transaction>> {
        self.inner.read().await.transaction(hash).await
    }

    async fn transaction_receipt(&self, hash: B256) -> Result<Option<Receipt>> {
        self.inner.read().await.transaction_receipt(hash).await
    }

    async fn transaction_block_reference(&self, hash: B256) -> Result<Option<(u64, B256, usize)>> {
        self.inner.read().await.transaction_block_reference(hash).await
    }
}

#[async_trait]
impl WriteProvider for StorageWriter {
    async fn add_block(&self, block: Block<Transaction>) -> Result<()> {
        self.inner.write().await.add_block(block);
        Ok(())
    }

    async fn add_transaction(&self, tx: Transaction) -> Result<()> {
        self.inner.write().await.add_transaction(tx);
        Ok(())
    }

    async fn add_receipt(&self, tx_hash: B256, receipt: Receipt) -> Result<()> {
        self.inner.write().await.add_receipt(tx_hash, receipt);
        Ok(())
    }

    async fn set_balance(&self, address: Address, balance: U256) -> Result<()> {
        self.inner.write().await.set_balance(address, balance);
        Ok(())
    }

    async fn add_payload(&self, payload_id: PayloadId, block: Block<Transaction>, receipts: Vec<Receipt>) -> Result<()> {
        self.inner.write().await.add_payload(payload_id, block, receipts);
        Ok(())
    }

    async fn remove_payload(&self, payload_id: &PayloadId) -> Result<Option<(Block<Transaction>, Vec<Receipt>)>> {
        Ok(self.inner.write().await.remove_payload(payload_id))
    }

    async fn update_forkchoice(&self, head: B256, safe: Option<B256>, finalized: Option<B256>) -> Result<()> {
        self.inner.write().await.update_forkchoice(head, safe, finalized);
        Ok(())
    }

    async fn revert_to_height(&self, height: u64) -> Result<Vec<Transaction>> {
        Ok(self.inner.write().await.revert_to_height(height))
    }
}

#[async_trait]
impl WriteProvider for StorageProvider {
    async fn add_block(&self, block: Block<Transaction>) -> Result<()> {
        self.writer.add_block(block).await
    }

    async fn add_transaction(&self, tx: Transaction) -> Result<()> {
        self.writer.add_transaction(tx).await
    }

    async fn add_receipt(&self, tx_hash: B256, receipt: Receipt) -> Result<()> {
        self.writer.add_receipt(tx_hash, receipt).await
    }

    async fn set_balance(&self, address: Address, balance: U256) -> Result<()> {
        self.writer.set_balance(address, balance).await
    }

    async fn add_payload(&self, payload_id: PayloadId, block: Block<Transaction>, receipts: Vec<Receipt>) -> Result<()> {
        self.writer.add_payload(payload_id, block, receipts).await
    }

    async fn remove_payload(&self, payload_id: &PayloadId) -> Result<Option<(Block<Transaction>, Vec<Receipt>)>> {
        self.writer.remove_payload(payload_id).await
    }

    async fn update_forkchoice(&self, head: B256, safe: Option<B256>, finalized: Option<B256>) -> Result<()> {
        self.writer.update_forkchoice(head, safe, finalized).await
    }

    async fn revert_to_height(&self, height: u64) -> Result<Vec<Transaction>> {
        self.writer.revert_to_height(height).await
    }
}
