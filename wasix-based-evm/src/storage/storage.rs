use crate::evm::ev::{H160, H256, EvmU256, evm, address_to_h160, alloy_u256_to_evm_u256, b256_to_h256, evm_u256_to_alloy_u256};
use std::time::Instant;
use crate::misc::metrics::{STORAGE_READ_LATENCY, STORAGE_WRITE_LATENCY, FORK_CHOICE_UPDATED_TOTAL, REORG_COUNT_TOTAL};
use crate::{info, debug, error};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, InMemoryAccount};
use alloy_primitives::{Address, B256, U256, Bytes, B64};
use alloy_trie::TrieAccount;
use alloy_trie::root::{state_root_unhashed, storage_root_unsorted};
use std::collections::{BTreeMap, HashMap};
use alloy_genesis::{Genesis, GenesisAccount, ChainConfig};
use crate::storage::traits::{StateProvider, WriteProvider};
use alloy_consensus::{Block, Header, ReceiptWithBloom as Receipt, TxEnvelope as Transaction};
use alloy_rpc_types::engine::PayloadId;
use alloy_eips::BlockId;
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use std::fs::File;
use std::path::Path;
use crate::evm::executor::Executor;
use crate::storage::mempool::Mempool;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GenesisInit {
    pub config: ChainConfig,
    pub alloc: BTreeMap<Address, GenesisAccount>,
    pub coinbase: Option<Address>,
    pub difficulty: Option<U256>,
    pub extra_data: Option<Bytes>,
    #[serde(default, with = "alloy_serde::quantity::opt")]
    pub gas_limit: Option<u64>,
    #[serde(default, with = "alloy_serde::quantity::opt")]
    pub nonce: Option<u64>,
    pub mixhash: Option<B256>,
    pub parent_hash: Option<B256>,
    #[serde(default, with = "alloy_serde::quantity::opt")]
    pub timestamp: Option<u64>,
    #[serde(default, with = "alloy_serde::quantity::opt")]
    pub number: Option<u64>,
}

impl From<GenesisInit> for Genesis {
    fn from(init: GenesisInit) -> Self {
        Self {
            config: init.config,
            alloc: init.alloc,
            coinbase: init.coinbase.unwrap_or_default(),
            difficulty: init.difficulty.unwrap_or_default(),
            extra_data: init.extra_data.unwrap_or_default(),
            gas_limit: init.gas_limit.unwrap_or_default(),
            nonce: init.nonce.unwrap_or_default(),
            mix_hash: init.mixhash.unwrap_or_default(),
            parent_hash: Some(init.parent_hash.unwrap_or_default()),
            timestamp: init.timestamp.unwrap_or_default(),
            number: init.number,
            base_fee_per_gas: None,
            excess_blob_gas: None,
            blob_gas_used: None,
        }
    }
}

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
    pub payloads: HashMap<PayloadId, (Block<Transaction>, Vec<Receipt>)>,
}

impl InMemoryStorage {
    pub fn new(chain_id: EvmU256) -> Self {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = evm_u256_to_alloy_u256(chain_id).to::<u64>();
        Self::new_with_genesis(chain_id, genesis)
    }

    pub fn new_with_genesis(chain_id: EvmU256, genesis: Genesis) -> Self {
        let (storage, genesis_block) = Self::create_from_genesis(chain_id, genesis);
        let mut storage = storage;
        let genesis_hash = genesis_block.header.hash_slow();
        
        // Ensure genesis hash is indexed
        storage.blocks.insert(genesis_block.header.number, genesis_block);
        storage.hash_to_number.insert(genesis_hash, 0);
        storage.head_block_hash = genesis_hash;
        storage.safe_block_hash = genesis_hash;
        storage.finalized_block_hash = genesis_hash;
        storage.snapshots.insert(0, storage.backend.clone());

        storage
    }

    pub fn new_with_genesis_init(chain_id: EvmU256, genesis: GenesisInit) -> Self {
        let (storage, genesis_block) = Self::create_from_genesis_init(chain_id, genesis);
        let mut storage = storage;
        let genesis_hash = genesis_block.header.hash_slow();
        
        // Ensure genesis hash is indexed
        storage.blocks.insert(genesis_block.header.number, genesis_block);
        storage.hash_to_number.insert(genesis_hash, 0);
        storage.head_block_hash = genesis_hash;
        storage.safe_block_hash = genesis_hash;
        storage.finalized_block_hash = genesis_hash;
        storage.snapshots.insert(0, storage.backend.clone());

        storage
    }

    pub fn create_from_genesis_init(chain_id: EvmU256, genesis: GenesisInit) -> (Self, Block<Transaction>) {
        let is_london = genesis.config.london_block == Some(0);
        let base_fee_per_gas = if is_london { Some(1_000_000_000) } else { None };
        
        let env = InMemoryEnvironment {
            block_hashes: BTreeMap::new(),
            block_number: EvmU256::zero(),
            block_coinbase: address_to_h160(genesis.coinbase.unwrap_or_default()),
            block_timestamp: EvmU256::from(genesis.timestamp.unwrap_or(0)),
            block_difficulty: alloy_u256_to_evm_u256(genesis.difficulty.unwrap_or_default()),
            block_randomness: Some(b256_to_h256(genesis.mixhash.unwrap_or_default())),
            block_gas_limit: EvmU256::from(genesis.gas_limit.unwrap_or_default()),
            block_base_fee_per_gas: alloy_u256_to_evm_u256(U256::from(base_fee_per_gas.unwrap_or(0))),
            blob_base_fee_per_gas: EvmU256::zero(),
            blob_versioned_hashes: vec![],
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
            payloads: HashMap::new(),
        };

        for (addr, account) in genesis.alloc {
            let mut storage_map = BTreeMap::new();
            if let Some(s) = account.storage {
                for (k, v) in s {
                    storage_map.insert(H256(k.0), H256(v.0));
                }
            }
            let im_account = InMemoryAccount {
                balance: alloy_u256_to_evm_u256(account.balance),
                nonce: EvmU256::from(account.nonce.unwrap_or(0)),
                code: account.code.map(|c| c.to_vec()).unwrap_or_default(),
                storage: storage_map,
                transient_storage: Default::default(),
            };
            let evm_addr = address_to_h160(addr);
            storage.backend.state.insert(evm_addr, im_account);
        }

        let state_root = storage.calculate_state_root();

        let is_shanghai = genesis.config.shanghai_time == Some(0);

        let genesis_block: Block<Transaction> = Block {
            header: Header {
                number: genesis.number.unwrap_or(0),
                timestamp: genesis.timestamp.unwrap_or(0),
                gas_limit: genesis.gas_limit.unwrap_or_default(),
                state_root,
                beneficiary: genesis.coinbase.unwrap_or_default(),
                difficulty: genesis.difficulty.unwrap_or_default(),
                mix_hash: genesis.mixhash.unwrap_or_default(),
                nonce: B64::from(genesis.nonce.unwrap_or_default()),
                base_fee_per_gas,
                extra_data: genesis.extra_data.unwrap_or_default(),
                transactions_root: alloy_trie::EMPTY_ROOT_HASH,
                receipts_root: alloy_trie::EMPTY_ROOT_HASH,
                withdrawals_root: if is_shanghai { Some(alloy_trie::EMPTY_ROOT_HASH) } else { None },
                gas_used: 0,
                parent_hash: genesis.parent_hash.unwrap_or_default(),
                ommers_hash: alloy_consensus::EMPTY_OMMER_ROOT_HASH,
                logs_bloom: Default::default(),
                blob_gas_used: None,
                excess_blob_gas: None,
                parent_beacon_block_root: None,
                requests_hash: None,
            },
            body: alloy_consensus::BlockBody {
                transactions: Vec::new(),
                ommers: Vec::new(),
                withdrawals: if is_shanghai { Some(alloy_eips::eip4895::Withdrawals::new(Vec::new())) } else { None },
            },
        };
        (storage, genesis_block)
    }

    pub fn create_from_genesis(chain_id: EvmU256, genesis: Genesis) -> (Self, Block<Transaction>) {
        let env = InMemoryEnvironment {
            block_hashes: BTreeMap::new(),
            block_number: EvmU256::zero(),
            block_coinbase: address_to_h160(genesis.coinbase),
            block_timestamp: EvmU256::from(genesis.timestamp),
            block_difficulty: alloy_u256_to_evm_u256(genesis.difficulty),
            block_randomness: Some(b256_to_h256(genesis.mix_hash)),
            block_gas_limit: EvmU256::from(genesis.gas_limit),
            block_base_fee_per_gas: alloy_u256_to_evm_u256(U256::from(1_000_000_000u64)),
            blob_base_fee_per_gas:  EvmU256::zero(),
            blob_versioned_hashes: vec![],
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
            payloads: HashMap::new(),
        };

        for (addr, account) in genesis.alloc {
            let mut storage_map = BTreeMap::new();
            if let Some(s) = account.storage {
                for (k, v) in s {
                    storage_map.insert(H256(k.0), H256(v.0));
                }
            }
            let im_account = InMemoryAccount {
                balance: alloy_u256_to_evm_u256(account.balance),
                nonce: EvmU256::from(account.nonce.unwrap_or(0)),
                code: account.code.map(|c| c.to_vec()).unwrap_or_default(),
                storage: storage_map,
                transient_storage: Default::default(),
            };
            let evm_addr = address_to_h160(addr);
            storage.backend.state.insert(evm_addr, im_account);
        }

        let state_root = storage.calculate_state_root();
        let genesis_block: Block<Transaction> = Block {
            header: Header {
                number: genesis.number.unwrap_or(0),
                timestamp: genesis.timestamp,
                gas_limit: genesis.gas_limit,
                state_root,
                beneficiary: genesis.coinbase,
                difficulty: genesis.difficulty,
                mix_hash: genesis.mix_hash,
                nonce: B64::from(genesis.nonce),
                base_fee_per_gas: None,
                extra_data: genesis.extra_data,
                transactions_root: alloy_trie::EMPTY_ROOT_HASH,
                receipts_root: alloy_trie::EMPTY_ROOT_HASH,
                withdrawals_root: Some(alloy_trie::EMPTY_ROOT_HASH),
                gas_used: 0,
                parent_hash: B256::ZERO,
                ommers_hash: alloy_consensus::EMPTY_OMMER_ROOT_HASH,
                logs_bloom: Default::default(),
                blob_gas_used: None,
                excess_blob_gas: None,
                parent_beacon_block_root: None,
                requests_hash: None,
            },
            body: alloy_consensus::BlockBody {
                transactions: Vec::new(),
                ommers: Vec::new(),
                withdrawals: Some(alloy_eips::eip4895::Withdrawals::new(Vec::new())),
            },
        };
        (storage, genesis_block)
    }

    pub fn new_with_genesis_block(_chain_id: EvmU256, storage: Self, genesis_block: Block<Transaction>) -> Self {
        let mut storage = storage;
        let genesis_hash = genesis_block.header.hash_slow();
        
        storage.blocks.insert(0, genesis_block);
        storage.hash_to_number.insert(genesis_hash, 0);
        storage.head_block_hash = genesis_hash;
        storage.safe_block_hash = genesis_hash;
        storage.finalized_block_hash = genesis_hash;
        storage.snapshots.insert(0, storage.backend.clone());

        storage
    }

    pub fn add_block(&mut self, block: Block<Transaction>) {
        let start = Instant::now();
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
        STORAGE_WRITE_LATENCY.observe(start.elapsed().as_secs_f64());
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

    pub fn add_payload(&mut self, payload_id: PayloadId, block: Block<Transaction>, receipts: Vec<Receipt>) {
        self.payloads.insert(payload_id, (block, receipts));
    }

    pub fn get_payload(&self, payload_id: &PayloadId) -> Option<&(Block<Transaction>, Vec<Receipt>)> {
        self.payloads.get(payload_id)
    }

    pub fn remove_payload(&mut self, payload_id: &PayloadId) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        self.payloads.remove(payload_id)
    }

    pub fn update_forkchoice(&mut self, head: B256, safe: Option<B256>, finalized: Option<B256>) {
        debug!("[Storage] Updating forkchoice: head={:?}, safe={:?}, finalized={:?}", head, safe, finalized);
        FORK_CHOICE_UPDATED_TOTAL.inc();
        
        // Simple reorg detection: if the new head is different and its parent is not our current head
        if self.head_block_hash != head && self.head_block_hash != B256::ZERO {
            if let Some(new_block) = self.get_block_by_hash(head) {
                if new_block.header.parent_hash != self.head_block_hash {
                    REORG_COUNT_TOTAL.inc();
                    info!("[Storage] Reorg detected! Old head: {:?}, New head: {:?}, New parent: {:?}", self.head_block_hash, head, new_block.header.parent_hash);
                }
            }
        }

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


#[async_trait]
impl StateProvider for InMemoryStorage {
    fn writer(&self) -> Arc<dyn WriteProvider> {
        unimplemented!("InMemoryStorage does not support WriteProvider directly")
    }

    async fn account(&self, address: Address, _block_id: BlockId) -> Result<Option<GenesisAccount>> {
        let start = Instant::now();
        let h160 = H160::from_slice(address.as_slice());
        let res = Ok(self.backend.state.get(&h160).map(|acc| GenesisAccount {
            balance: {
                let mut b = [0u8; 32];
                acc.balance.to_big_endian(&mut b);
                U256::from_be_bytes(b)
            },
            nonce: Some(acc.nonce.as_u64()),
            code: if acc.code.is_empty() { None } else { Some(acc.code.clone().into()) },
            storage: if acc.storage.is_empty() { None } else {
                Some(acc.storage.iter().map(|(k, v)| (B256::from(k.0), B256::from(v.0))).collect())
            },
            private_key: None,
        }));
        STORAGE_READ_LATENCY.observe(start.elapsed().as_secs_f64());
        res
    }

    async fn storage(&self, address: Address, slot: B256, _block_id: BlockId) -> Result<Option<U256>> {
        let start = Instant::now();
        let h160 = H160::from_slice(address.as_slice());
        let h256 = H256(slot.0);
        let res = Ok(self.backend.state.get(&h160)
            .and_then(|acc| acc.storage.get(&h256))
            .map(|v| U256::from_be_bytes(v.0)));
        STORAGE_READ_LATENCY.observe(start.elapsed().as_secs_f64());
        res
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
        Ok(evm_u256_to_alloy_u256(chain_id).to::<u64>())
    }

    async fn logs(&self, _filter: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::eth::Log>> {
        // TODO use the filter on indexed logs
        Ok(vec![])
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;
    use alloy_consensus::SignableTransaction;

    #[tokio::test]
    async fn test_new_storage() {
        let chain_id = EvmU256::from(1);
        let storage = InMemoryStorage::new(chain_id);
        assert_eq!(storage.backend.environment.chain_id, chain_id);
        assert_eq!(storage.get_latest_block_number(), 0);
    }

    #[tokio::test]
    async fn test_balance_management() {
        let mut storage = InMemoryStorage::new(EvmU256::from(1));
        let addr = address!("0000000000000000000000000000000000000001");
        let balance = U256::from(1000);
        
        storage.set_balance(addr, balance);
        assert_eq!(storage.get_balance(addr), balance);
        
        let provider_balance = storage.balance(addr, BlockId::latest()).await.unwrap();
        assert_eq!(provider_balance, balance);
    }
    
    #[tokio::test]
    async fn test_genesis_init() {
        let chain_id = EvmU256::from(1);
        let mut alloc = BTreeMap::new();
        let addr = address!("0000000000000000000000000000000000000001");
        alloc.insert(addr, GenesisAccount {
            balance: U256::from(1000),
            nonce: Some(1),
            ..Default::default()
        });
        
        let genesis_init = GenesisInit {
            alloc,
            config: ChainConfig::default(),
            coinbase: None,
            difficulty: None,
            extra_data: None,
            gas_limit: None,
            nonce: None,
            mixhash: None,
            parent_hash: None,
            timestamp: None,
            number: None,
        };
        
        let (storage, _block) = InMemoryStorage::create_from_genesis_init(chain_id, genesis_init);
        assert_eq!(storage.get_balance(addr), U256::from(1000));
        let acc = storage.account(addr, BlockId::latest()).await.unwrap().unwrap();
        assert_eq!(acc.nonce, Some(1));
    }

    #[tokio::test]
    async fn test_block_management() {
        let mut storage = InMemoryStorage::new(EvmU256::from(1));
        let mut block: Block<Transaction> = Block::default();
        block.header.number = 1;
        let block_hash = block.header.hash_slow();
        
        storage.add_block(block.clone());
        assert_eq!(storage.get_latest_block_number(), 1);
        assert_eq!(storage.get_block_by_number(1).unwrap().header.number, 1);
        assert_eq!(storage.get_block_by_hash(block_hash).unwrap().header.number, 1);
        
        storage.revert_to_height(0);
        assert_eq!(storage.get_latest_block_number(), 0);
        assert!(storage.get_block_by_number(1).is_none());
    }

    #[tokio::test]
    async fn test_transaction_and_receipt() {
        let mut storage = InMemoryStorage::new(EvmU256::from(1));
        let tx = Transaction::Legacy(alloy_consensus::TxLegacy::default().into_signed(alloy_primitives::Signature::test_signature()));
        let tx_hash = *tx.hash();
        
        storage.add_transaction(tx.clone());
        assert_eq!(storage.get_transaction_by_hash(tx_hash).unwrap().hash(), &tx_hash);
        
        let receipt = Receipt::default();
        storage.add_receipt(tx_hash, receipt.clone());
        assert!(storage.get_receipt_by_tx_hash(tx_hash).is_some());
    }

    #[tokio::test]
    async fn test_forkchoice_update() {
        let mut storage = InMemoryStorage::new(EvmU256::from(1));
        let h1 = B256::repeat_byte(1);
        let h2 = B256::repeat_byte(2);
        let h3 = B256::repeat_byte(3);
        
        storage.update_forkchoice(h1, Some(h2), Some(h3));
        assert_eq!(storage.head_block_hash, h1);
        assert_eq!(storage.safe_block_hash, h2);
        assert_eq!(storage.finalized_block_hash, h3);
    }
}


