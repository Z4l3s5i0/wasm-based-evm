use crate::ev::{H160, H256, EvmU256, evm, address_to_h160, alloy_u256_to_evm_u256, b256_to_h256};
use crate::{info, debug, error};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, InMemoryAccount};
use alloy_primitives::{Address, B256, U256, Bytes, B64};
use alloy_trie::TrieAccount;
use alloy_trie::root::{state_root_unhashed, storage_root_unsorted};
use std::collections::BTreeMap;
use crate::storage::genesis::Genesis;
use crate::storage::traits::{StateProvider, BlockProvider, TransactionProvider, LogProvider};
use crate::mempool::Mempool;
use alloy_consensus::{Block, ReceiptWithBloom as Receipt, TxEnvelope as Transaction, Header};
use alloy_eips::BlockId;
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use std::fs::File;
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
pub struct InMemoryStorage {
    pub backend: InMemoryBackend,
    pub blocks: BTreeMap<u64, Block<Transaction>>,
    pub hash_to_number: BTreeMap<B256, u64>,
    pub transactions: BTreeMap<B256, Transaction>,
    pub receipts: BTreeMap<B256, Receipt>,
    pub tx_location: BTreeMap<B256, (u64, B256, usize)>, // hash -> (number, hash, index)
    pub head_block_hash: B256,
    pub safe_block_hash: B256,
    pub finalized_block_hash: B256,
    pub snapshots: BTreeMap<u64, InMemoryBackend>,
}

impl InMemoryStorage {
    pub fn new(chain_id: EvmU256) -> Self {
        Self::new_with_genesis(chain_id, Genesis::default())
    }

    pub fn new_with_genesis(chain_id: EvmU256, genesis: Genesis) -> Self {
        let env = InMemoryEnvironment {
            block_hashes: BTreeMap::new(),
            block_number: EvmU256::zero(),
            block_coinbase: address_to_h160(genesis.coinbase),
            block_timestamp: EvmU256::from(genesis.timestamp),
            block_difficulty: alloy_u256_to_evm_u256(genesis.difficulty),
            block_randomness: Some(b256_to_h256(genesis.mix_hash)),
            block_gas_limit: EvmU256::from(genesis.gas_limit),
            block_base_fee_per_gas: alloy_u256_to_evm_u256(genesis.base_fee_per_gas.unwrap_or_else(|| U256::from(1_000_000_000u64))),
            blob_base_fee_per_gas: EvmU256::zero(),
            blob_versioned_hashes: Vec::new(),
            chain_id,
        };

        let mut storage = Self {
            backend: InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            },
            blocks: BTreeMap::new(),
            hash_to_number: BTreeMap::new(),
            transactions: BTreeMap::new(),
            receipts: BTreeMap::new(),
            tx_location: BTreeMap::new(),
            head_block_hash: B256::ZERO,
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
            snapshots: BTreeMap::new(),
        };

        for account in genesis.accounts {
            let mut storage_map = BTreeMap::new();
            if let Some(s) = account.storage {
                for (k, v) in s {
                    storage_map.insert(H256(k.0), H256(v.0));
                }
            }
            let im_account = InMemoryAccount {
                balance: alloy_u256_to_evm_u256(account.balance),
                nonce: EvmU256::from(account.nonce.unwrap_or(0)),
                code: account.code.unwrap_or_default(),
                storage: storage_map,
                transient_storage: Default::default(),
            };
            let addr = address_to_h160(account.address);
            info!("[Storage] Initializing account {:?}: balance={}, nonce={}, code_len={}, storage_len={}", 
                addr, im_account.balance, im_account.nonce, im_account.code.len(), im_account.storage.len());
            storage.backend.state.insert(addr, im_account);
        }

        let state_root = storage.calculate_state_root();
        info!("[Storage] Calculated genesis state root: {:?}", state_root);
        let genesis_block: Block<Transaction> = Block {
            header: Header {
                number: 0,
                timestamp: genesis.timestamp,
                gas_limit: genesis.gas_limit,
                state_root,
                beneficiary: genesis.coinbase,
                difficulty: genesis.difficulty,
                mix_hash: genesis.mix_hash,
                nonce: B64::ZERO, // Nonce is meaningless post-merge, usually 0x0
                base_fee_per_gas: Some(genesis.base_fee_per_gas.map(|v| v.to::<u64>()).unwrap_or(1_000_000_000)),
                extra_data: genesis.extra_data.into(),
                transactions_root: alloy_trie::EMPTY_ROOT_HASH,
                receipts_root: alloy_trie::EMPTY_ROOT_HASH,
                withdrawals_root: Some(alloy_trie::EMPTY_ROOT_HASH),
                gas_used: 0,
                parent_hash: B256::ZERO,
                ommers_hash: alloy_consensus::EMPTY_OMMER_ROOT_HASH,
                logs_bloom: Default::default(),
                blob_gas_used: None,
                excess_blob_gas: None,
                parent_beacon_block_root: Some(B256::ZERO),
                requests_hash: None,
            },
            body: alloy_consensus::BlockBody {
                transactions: Vec::new(),
                ommers: Vec::new(),
                withdrawals: Some(alloy_eips::eip4895::Withdrawals::new(Vec::new())),
            },
        };
        let genesis_hash = genesis_block.header.hash_slow();
        info!("[Storage] Calculated genesis hash: {:?}", genesis_hash);
        
        // Also log some key fields to help debug CL/EL mismatches
        info!("[Storage] Genesis header RLP fields: state_root={:?}, transactions_root={:?}, receipts_root={:?}, withdrawals_root={:?}, parent_beacon_block_root={:?}", 
            genesis_block.header.state_root, genesis_block.header.transactions_root, genesis_block.header.receipts_root, genesis_block.header.withdrawals_root, genesis_block.header.parent_beacon_block_root);
        
        info!("[Storage] Genesis header fields: nonce={:?}, mix_hash={:?}, difficulty={:?}, coinbase={:?}, timestamp={:?}, gas_limit={:?}, base_fee={:?}", 
            genesis.nonce, genesis.mix_hash, genesis.difficulty, genesis.coinbase, genesis.timestamp, genesis.gas_limit, genesis.base_fee_per_gas);
        
        // Ensure genesis hash is indexed
        storage.blocks.insert(0, genesis_block);
        storage.hash_to_number.insert(genesis_hash, 0);
        storage.head_block_hash = genesis_hash;
        storage.safe_block_hash = genesis_hash;
        storage.finalized_block_hash = genesis_hash;
        storage.snapshots.insert(0, storage.backend.clone());

        storage
    }

    pub fn add_block(&mut self, block: Block<Transaction>) {
        let block_number = block.header.number;
        let block_hash = block.header.hash_slow();
        
        // Check if we already have this block
        if self.hash_to_number.contains_key(&block_hash) {
            debug!("[Storage] Block #{} with hash {:?} already exists, skipping", block_number, block_hash);
            return;
        }

        info!("[Storage] Adding block #{} with hash {:?}", block_number, block_hash);
        
        for (i, tx) in block.body.transactions.iter().enumerate() {
            let tx_hash = tx.hash();
            debug!("[Storage] Indexing transaction {:?} in block {}", tx_hash, block_number);
            self.tx_location.insert(*tx_hash, (block_number, block_hash, i));
            self.transactions.insert(*tx_hash, tx.clone());
        }

        self.blocks.insert(block_number, block);
        self.hash_to_number.insert(block_hash, block_number);
        self.head_block_hash = block_hash;
        
        // Take snapshot after adding block
        self.snapshots.insert(block_number, self.backend.clone());
        // Keep only last 100 snapshots
        if self.snapshots.len() > 100 {
            if let Some(&first) = self.snapshots.keys().next() {
                self.snapshots.remove(&first);
            }
        }
    }

    pub fn revert_to_height(&mut self, height: u64) -> Vec<Transaction> {
        info!("[Storage] Reverting to height {}", height);
        let mut reverted_txs = Vec::new();
        
        let keys_to_remove: Vec<u64> = self.blocks.range((height + 1)..).map(|(k, _)| *k).collect();
        for k in keys_to_remove {
            if let Some(block) = self.blocks.remove(&k) {
                let block_hash = block.header.hash_slow();
                self.hash_to_number.remove(&block_hash);
                for tx in block.body.transactions {
                    let hash = tx.hash();
                    self.tx_location.remove(hash);
                    // We don't necessarily remove from self.transactions if we want to keep them for mempool
                    reverted_txs.push(tx);
                }
            }
        }
        
        if let Some(snapshot) = self.snapshots.get(&height) {
            self.backend = snapshot.clone();
        } else {
            error!("[Storage] No snapshot found for height {}, state might be inconsistent!", height);
        }
        
        if let Some(block) = self.blocks.get(&height) {
            self.head_block_hash = block.header.hash_slow();
        }
        
        reverted_txs
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        let hash = tx.hash();
        debug!("[Storage] Adding transaction {:?}", hash);
        self.transactions.insert(*hash, tx);
    }

    pub fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        debug!("[Storage] Adding receipt for transaction {:?}", tx_hash);
        self.receipts.insert(tx_hash, receipt);
    }

    pub fn get_receipt_by_tx_hash(&self, tx_hash: B256) -> Option<&Receipt> {
        self.receipts.get(&tx_hash)
    }

    pub fn get_block_by_number(&self, number: u64) -> Option<&Block<Transaction>> {
        self.blocks.get(&number)
    }

    pub fn get_block_by_hash(&self, hash: B256) -> Option<&Block<Transaction>> {
        self.hash_to_number.get(&hash).and_then(|&num| self.blocks.get(&num))
    }

    pub fn get_transaction_by_hash(&self, hash: B256) -> Option<&Transaction> {
        self.transactions.get(&hash)
    }

    pub fn get_block_by_transaction_hash(&self, tx_hash: B256) -> Option<&Block<Transaction>> {
        self.tx_location.get(&tx_hash).and_then(|(num, _, _)| self.blocks.get(num))
    }

    pub fn get_block_receipts(&self, block_hash: B256) -> Vec<Receipt> {
        let block = match self.get_block_by_hash(block_hash) {
            Some(b) => b,
            None => return Vec::new(),
        };
        block.body.transactions.iter()
            .filter_map(|tx| self.receipts.get(tx.hash()).cloned())
            .collect()
    }

    pub fn get_latest_block(&self) -> Option<&Block<Transaction>> {
        self.blocks.values().last()
    }

    pub fn get_latest_block_number(&self) -> u64 {
        self.blocks.keys().last().cloned().unwrap_or(0)
    }

    /// Get a mutable reference to the backend.
    pub fn backend_mut(&mut self) -> &mut InMemoryBackend {
        &mut self.backend
    }

    /// Set the backend.
    pub fn set_backend(&mut self, backend: InMemoryBackend) {
        self.backend = backend;
    }

    pub fn get_balance(&self, address: Address) -> U256 {
        let h160 = H160::from_slice(address.as_slice());
        if let Some(a) = self.backend.state.get(&h160) {
            let mut bytes = [0u8; 32];
            a.balance.to_big_endian(&mut bytes);
            U256::from_be_bytes(bytes)
        } else {
            U256::ZERO
        }
    }

    pub fn get_accounts(&self) -> Vec<Address> {
        self.backend.state.keys().map(|h| Address::from(h.0)).collect()
    }

    pub fn get_code(&self, address: Address) -> Vec<u8> {
        self.backend.state.get(&H160::from_slice(address.as_slice()))
            .map(|a| a.code.clone())
            .unwrap_or_default()
    }



    pub fn set_balance(&mut self, address: Address, balance: U256) {
        debug!("[Storage] Setting balance for address {:?} to {}", address, balance);
        let bytes = balance.to_be_bytes::<32>();
        let evm_balance = EvmU256::from_big_endian(&bytes);
        let h160 = H160::from_slice(address.as_slice());
        self.backend.state.entry(h160).or_insert(InMemoryAccount {
            balance: evm_balance,
            code: Vec::new(),
            nonce: EvmU256::zero(),
            storage: BTreeMap::<H256, H256>::new(),
            transient_storage: BTreeMap::<H256, H256>::new(),
        }).balance = evm_balance;
    }

    pub fn calculate_state_root(&self) -> B256 {
        state_root_unhashed(self.backend.state.iter().map(|(addr, acc)| {
            let storage_root = if acc.storage.is_empty() {
                alloy_trie::EMPTY_ROOT_HASH
            } else {
                storage_root_unsorted(acc.storage.iter().map(|(k, v)| {
                    (alloy_primitives::keccak256(k.0), U256::from_be_bytes(v.0))
                }))
            };

            let trie_acc = TrieAccount {
                nonce: acc.nonce.as_u64(),
                balance: {
                    let mut b = [0u8; 32];
                    acc.balance.to_big_endian(&mut b);
                    U256::from_be_bytes(b)
                },
                storage_root,
                code_hash: if acc.code.is_empty() { 
                    alloy_primitives::KECCAK256_EMPTY 
                } else { 
                    alloy_primitives::keccak256(&acc.code) 
                },
            };
            (Address::from(addr.0), trie_acc)
        }))
    }

    pub fn get_block_hash(&self, number: u64) -> Option<B256> {
        self.get_block_by_number(number).map(|b| b.header.hash_slow())
    }

    pub fn get_block_by_id(&self, block_id: BlockId) -> Option<&Block<Transaction>> {
        match block_id {
            BlockId::Hash(hash) => self.get_block_by_hash(hash.into()),
            BlockId::Number(num) => {
                match num {
                    alloy_eips::BlockNumberOrTag::Number(n) => self.get_block_by_number(n),
                    alloy_eips::BlockNumberOrTag::Latest | alloy_eips::BlockNumberOrTag::Pending => {
                        self.get_block_by_hash(self.head_block_hash)
                    }
                    alloy_eips::BlockNumberOrTag::Safe => self.get_block_by_hash(self.safe_block_hash),
                    alloy_eips::BlockNumberOrTag::Finalized => {
                        self.get_block_by_hash(self.finalized_block_hash)
                    }
                    alloy_eips::BlockNumberOrTag::Earliest => self.get_block_by_number(0),
                }
            }
        }
    }

    pub fn get_block_hash_by_id(&self, block_id: BlockId) -> Option<B256> {
        match block_id {
            BlockId::Hash(hash) => Some(hash.into()),
            BlockId::Number(num) => match num {
                alloy_eips::BlockNumberOrTag::Number(n) => self.get_block_hash(n),
                alloy_eips::BlockNumberOrTag::Latest | alloy_eips::BlockNumberOrTag::Pending => {
                    Some(self.head_block_hash)
                }
                alloy_eips::BlockNumberOrTag::Safe => Some(self.safe_block_hash),
                alloy_eips::BlockNumberOrTag::Finalized => Some(self.finalized_block_hash),
                alloy_eips::BlockNumberOrTag::Earliest => self.get_block_hash(0),
            },
        }
    }

    pub fn update_forkchoice(&mut self, head: B256, safe: Option<B256>, finalized: Option<B256>) {
        info!("[Storage] Updating forkchoice: head={:?}, safe={:?}, finalized={:?}", head, safe, finalized);
        self.head_block_hash = head;
        if let Some(s) = safe {
            self.safe_block_hash = s;
        }
        if let Some(f) = finalized {
            self.finalized_block_hash = f;
        }
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let storage = serde_json::from_reader(file)?;
        Ok(storage)
    }
}

pub struct StorageProvider {
    inner: Arc<RwLock<InMemoryStorage>>,
    mempool: Arc<RwLock<Mempool>>,
    executor: Arc<crate::executor::Executor>,
}

impl StorageProvider {
    pub fn new(
        storage: Arc<RwLock<InMemoryStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        executor: Arc<crate::executor::Executor>,
    ) -> Self {
        Self {
            inner: storage,
            mempool,
            executor,
        }
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
                ..Default::default()
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
    async fn account(&self, address: Address, block_id: BlockId) -> Result<Option<crate::storage::genesis::GenesisAccount>> {
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
}

#[async_trait]
impl BlockProvider for StorageProvider {
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
}

#[async_trait]
impl TransactionProvider for StorageProvider {
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
impl LogProvider for StorageProvider {
    async fn logs(&self, filter: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::Log>> {
        self.inner.read().await.logs(filter).await
    }
}

#[async_trait]
impl StateProvider for InMemoryStorage {
    async fn account(&self, address: Address, _block_id: BlockId) -> Result<Option<crate::storage::genesis::GenesisAccount>> {
        let h160 = H160::from_slice(address.as_slice());
        Ok(self.backend.state.get(&h160).map(|acc| crate::storage::genesis::GenesisAccount {
            address,
            balance: {
                let mut b = [0u8; 32];
                acc.balance.to_big_endian(&mut b);
                U256::from_be_bytes(b)
            },
            nonce: Some(acc.nonce.as_u64()),
            code: Some(acc.code.clone()),
            storage: Some(acc.storage.iter().map(|(k, v)| (B256::from(k.0), B256::from(v.0))).collect()),
        }))
    }

    async fn storage(&self, address: Address, slot: B256, _block_id: BlockId) -> Result<Option<U256>> {
        let h160 = H160::from_slice(address.as_slice());
        let h256 = H256(slot.0);
        Ok(self.backend.state.get(&h160)
            .and_then(|acc| acc.storage.get(&h256))
            .map(|v| U256::from_be_bytes(v.0)))
    }

    async fn code(&self, address: Address, _block_id: BlockId) -> Result<Option<Bytes>> {
        let h160 = H160::from_slice(address.as_slice());
        Ok(self.backend.state.get(&h160).map(|acc| acc.code.clone().into()))
    }

    async fn balance(&self, address: Address, _block_id: BlockId) -> Result<U256> {
        Ok(self.get_balance(address))
    }

    async fn transaction_count(&self, address: Address, _block_id: BlockId) -> Result<u64> {
        let h160 = H160::from_slice(address.as_slice());
        Ok(self.backend.state.get(&h160).map(|acc| acc.nonce.as_u64()).unwrap_or(0))
    }

    async fn accounts(&self) -> Result<Vec<Address>> {
        Ok(self.get_accounts())
    }
}

#[async_trait]
impl BlockProvider for InMemoryStorage {
    async fn header(&self, block_id: BlockId) -> Result<Option<Header>> {
        Ok(self.get_block_by_id(block_id).map(|b| b.header.clone()))
    }

    async fn block(&self, block_id: BlockId) -> Result<Option<Block<Transaction>>> {
        Ok(self.get_block_by_id(block_id).cloned())
    }

    async fn block_hash(&self, number: u64) -> Result<Option<B256>> {
        Ok(self.get_block_hash(number))
    }

    async fn latest_block_number(&self) -> Result<u64> {
        Ok(self.get_latest_block_number())
    }

    async fn chain_id(&self) -> Result<u64> {
        let chain_id = self.backend.environment.chain_id;
        Ok(crate::ev::evm_u256_to_alloy_u256(chain_id).to::<u64>())
    }
}

#[async_trait]
impl TransactionProvider for InMemoryStorage {
    async fn transaction(&self, hash: B256) -> Result<Option<Transaction>> {
        Ok(self.get_transaction_by_hash(hash).cloned())
    }

    async fn transaction_receipt(&self, hash: B256) -> Result<Option<Receipt>> {
        Ok(self.get_receipt_by_tx_hash(hash).cloned())
    }

    async fn transaction_block_reference(&self, hash: B256) -> Result<Option<(u64, B256, usize)>> {
        Ok(self.tx_location.get(&hash).cloned())
    }
}

#[async_trait]
impl LogProvider for InMemoryStorage {
    async fn logs(&self, _filter: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::Log>> {
        // TODO use the filter on indexed logs
        Ok(vec![])
    }
}
