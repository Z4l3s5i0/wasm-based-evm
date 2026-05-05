use crate::evm::ev::{H160, H256, EvmU256, address_to_h160, alloy_u256_to_evm_u256, b256_to_h256, evm_u256_to_alloy_u256};
use std::time::Instant;
use crate::misc::metrics::{STORAGE_READ_LATENCY, STORAGE_WRITE_LATENCY, FORK_CHOICE_UPDATED_TOTAL, REORG_COUNT_TOTAL};
use crate::{info, debug, error};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, InMemoryAccount, OverlayedChangeSet};
use alloy_primitives::{Address, B256, U256, Bytes, B64};
use alloy_trie::TrieAccount;
use alloy_trie::root::{state_root_unhashed, storage_root_unsorted};
use std::collections::{BTreeMap, HashMap};
use alloy_genesis::{Genesis, GenesisAccount, ChainConfig};
use crate::storage::traits::{StateProvider, WriteProvider, SyncStateProvider, StateSnapshot, Database};
use alloy_consensus::{Block, Header, ReceiptWithBloom as Receipt, TxEnvelope as Transaction};
use alloy_rpc_types::engine::PayloadId;
use alloy_eips::BlockId;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::any::Any;

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
pub struct StateStore {
    pub backend: InMemoryBackend,
    pub snapshots: BTreeMap<u64, InMemoryBackend>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChainStore {
    pub blocks: BTreeMap<u64, Block<Transaction>>,
    pub hash_to_number: BTreeMap<B256, u64>,
    pub transactions: BTreeMap<B256, Transaction>,
    pub receipts: BTreeMap<B256, Receipt>,
    pub tx_location: BTreeMap<B256, (u64, B256, usize)>, // hash -> (number, hash, index)
    pub head_block_hash: B256,
    pub safe_block_hash: B256,
    pub finalized_block_hash: B256,
    pub payloads: HashMap<PayloadId, (Block<Transaction>, Vec<Receipt>)>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InMemoryStorage {
    pub state: StateStore,
    pub chain: ChainStore,
}

impl InMemoryStorage {
    pub fn new(chain_id: EvmU256) -> Self {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = evm_u256_to_alloy_u256(chain_id).to::<u64>();
        Self::new_with_genesis(chain_id, genesis)
    }

    pub fn new_with_genesis(chain_id: EvmU256, genesis: Genesis) -> Self {
        let (mut storage, genesis_block) = Self::create_from_genesis(chain_id, genesis);
        let genesis_hash = genesis_block.header.hash_slow();
        
        storage.chain.blocks.insert(genesis_block.header.number, genesis_block);
        storage.chain.hash_to_number.insert(genesis_hash, 0);
        storage.chain.head_block_hash = genesis_hash;
        storage.chain.safe_block_hash = genesis_hash;
        storage.chain.finalized_block_hash = genesis_hash;
        storage.state.snapshots.insert(0, storage.state.backend.clone());

        storage
    }

    pub fn new_with_genesis_init(chain_id: EvmU256, genesis: GenesisInit) -> Self {
        let (mut storage, genesis_block) = Self::create_from_genesis_init(chain_id, genesis);
        let genesis_hash = genesis_block.header.hash_slow();
        
        storage.chain.blocks.insert(genesis_block.header.number, genesis_block);
        storage.chain.hash_to_number.insert(genesis_hash, 0);
        storage.chain.head_block_hash = genesis_hash;
        storage.chain.safe_block_hash = genesis_hash;
        storage.chain.finalized_block_hash = genesis_hash;
        storage.state.snapshots.insert(0, storage.state.backend.clone());

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

        let mut state = StateStore {
            backend: InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            },
            snapshots: BTreeMap::new(),
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
            state.backend.state.insert(evm_addr, im_account);
        }

        let mut storage = Self {
            state,
            chain: ChainStore {
                blocks: BTreeMap::new(),
                hash_to_number: BTreeMap::new(),
                transactions: BTreeMap::new(),
                receipts: BTreeMap::new(),
                tx_location: BTreeMap::new(),
                head_block_hash: B256::ZERO,
                safe_block_hash: B256::ZERO,
                finalized_block_hash: B256::ZERO,
                payloads: HashMap::new(),
            },
        };

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

        let mut state = StateStore {
            backend: InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            },
            snapshots: BTreeMap::new(),
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
            state.backend.state.insert(evm_addr, im_account);
        }

        let mut storage = Self {
            state,
            chain: ChainStore {
                blocks: BTreeMap::new(),
                hash_to_number: BTreeMap::new(),
                transactions: BTreeMap::new(),
                receipts: BTreeMap::new(),
                tx_location: BTreeMap::new(),
                head_block_hash: B256::ZERO,
                safe_block_hash: B256::ZERO,
                finalized_block_hash: B256::ZERO,
                payloads: HashMap::new(),
            },
        };

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

    pub fn new_with_genesis_block(_chain_id: EvmU256, mut storage: Self, genesis_block: Block<Transaction>) -> Self {
        let genesis_hash = genesis_block.header.hash_slow();
        
        storage.chain.blocks.insert(0, genesis_block);
        storage.chain.hash_to_number.insert(genesis_hash, 0);
        storage.chain.head_block_hash = genesis_hash;
        storage.chain.safe_block_hash = genesis_hash;
        storage.chain.finalized_block_hash = genesis_hash;
        storage.state.snapshots.insert(0, storage.state.backend.clone());

        storage
    }

    pub fn add_block(&mut self, block: Block<Transaction>) {
        let start = Instant::now();
        let block_number = block.header.number;
        let block_hash = block.header.hash_slow();
        
        if self.chain.hash_to_number.contains_key(&block_hash) {
            debug!("[Storage] Block #{} with hash {:?} already exists, skipping", block_number, block_hash);
            return;
        }

        info!("[Storage] Adding block #{} with hash {:?}", block_number, block_hash);
        
        for (i, tx) in block.body.transactions.iter().enumerate() {
            let tx_hash = tx.hash();
            debug!("[Storage] Indexing transaction {:?} in block {}", tx_hash, block_number);
            self.chain.tx_location.insert(*tx_hash, (block_number, block_hash, i));
            self.chain.transactions.insert(*tx_hash, tx.clone());
        }

        self.chain.blocks.insert(block_number, block);
        self.chain.hash_to_number.insert(block_hash, block_number);
        self.chain.head_block_hash = block_hash;
        
        self.state.snapshots.insert(block_number, self.state.backend.clone());
        if self.state.snapshots.len() > 100 {
            if let Some(&first) = self.state.snapshots.keys().next() {
                self.state.snapshots.remove(&first);
            }
        }
        STORAGE_WRITE_LATENCY.observe(start.elapsed().as_secs_f64());
    }

    pub fn revert_to_height(&mut self, height: u64) -> Vec<Transaction> {
        info!("[Storage] Reverting to height {}", height);
        let mut reverted_txs = Vec::new();
        
        let keys_to_remove: Vec<u64> = self.chain.blocks.range((height + 1)..).map(|(k, _)| *k).collect();
        for k in keys_to_remove {
            if let Some(block) = self.chain.blocks.remove(&k) {
                let block_hash = block.header.hash_slow();
                self.chain.hash_to_number.remove(&block_hash);
                for tx in block.body.transactions {
                    let hash = tx.hash();
                    self.chain.tx_location.remove(hash);
                    reverted_txs.push(tx);
                }
            }
        }
        
        if let Some(snapshot) = self.state.snapshots.get(&height) {
            self.state.backend = snapshot.clone();
        } else {
            error!("[Storage] No snapshot found for height {}, state might be inconsistent!", height);
        }
        
        if let Some(block) = self.chain.blocks.get(&height) {
            self.chain.head_block_hash = block.header.hash_slow();
        }
        
        reverted_txs
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        let hash = tx.hash();
        debug!("[Storage] Adding transaction {:?}", hash);
        self.chain.transactions.insert(*hash, tx);
    }

    pub fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        debug!("[Storage] Adding receipt for transaction {:?}", tx_hash);
        self.chain.receipts.insert(tx_hash, receipt);
    }

    pub fn get_receipt_by_tx_hash(&self, tx_hash: B256) -> Option<&Receipt> {
        self.chain.receipts.get(&tx_hash)
    }

    pub fn get_block_by_number(&self, number: u64) -> Option<&Block<Transaction>> {
        self.chain.blocks.get(&number)
    }

    pub fn get_block_by_hash(&self, hash: B256) -> Option<&Block<Transaction>> {
        self.chain.hash_to_number.get(&hash).and_then(|&num| self.chain.blocks.get(&num))
    }

    pub fn get_transaction_by_hash(&self, hash: B256) -> Option<&Transaction> {
        self.chain.transactions.get(&hash)
    }

    pub fn get_block_by_transaction_hash(&self, tx_hash: B256) -> Option<&Block<Transaction>> {
        self.chain.tx_location.get(&tx_hash).and_then(|(num, _, _)| self.chain.blocks.get(num))
    }

    pub fn get_block_receipts(&self, block_hash: B256) -> Vec<Receipt> {
        let block = match self.get_block_by_hash(block_hash) {
            Some(b) => b,
            None => return Vec::new(),
        };
        block.body.transactions.iter()
            .filter_map(|tx| self.chain.receipts.get(tx.hash()).cloned())
            .collect()
    }

    pub fn get_latest_block(&self) -> Option<&Block<Transaction>> {
        self.chain.blocks.values().last()
    }

    pub fn get_latest_block_number(&self) -> u64 {
        self.chain.blocks.keys().last().cloned().unwrap_or(0)
    }

    pub fn get_balance(&self, address: Address) -> U256 {
        let h160 = H160::from_slice(address.as_slice());
        if let Some(a) = self.state.backend.state.get(&h160) {
            let mut bytes = [0u8; 32];
            a.balance.to_big_endian(&mut bytes);
            U256::from_be_bytes(bytes)
        } else {
            U256::ZERO
        }
    }

    pub fn get_accounts(&self) -> Vec<Address> {
        self.state.backend.state.keys().map(|h| Address::from(h.0)).collect()
    }

    pub fn get_code(&self, address: Address) -> Vec<u8> {
        self.state.backend.state.get(&H160::from_slice(address.as_slice()))
            .map(|a| a.code.clone())
            .unwrap_or_default()
    }

    pub fn set_balance(&mut self, address: Address, balance: U256) {
        debug!("[Storage] Setting balance for address {:?} to {}", address, balance);
        let bytes = balance.to_be_bytes::<32>();
        let evm_balance = EvmU256::from_big_endian(&bytes);
        let h160 = H160::from_slice(address.as_slice());
        self.state.backend.state.entry(h160).or_insert(InMemoryAccount {
            balance: evm_balance,
            code: Vec::new(),
            nonce: EvmU256::zero(),
            storage: BTreeMap::<H256, H256>::new(),
            transient_storage: BTreeMap::<H256, H256>::new(),
        }).balance = evm_balance;
    }

    pub fn calculate_state_root(&self) -> B256 {
        state_root_unhashed(self.state.backend.state.iter().map(|(addr, acc)| {
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
                        self.get_block_by_hash(self.chain.head_block_hash)
                    }
                    alloy_eips::BlockNumberOrTag::Safe => self.get_block_by_hash(self.chain.safe_block_hash),
                    alloy_eips::BlockNumberOrTag::Finalized => {
                        self.get_block_by_hash(self.chain.finalized_block_hash)
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
                    Some(self.chain.head_block_hash)
                }
                alloy_eips::BlockNumberOrTag::Safe => Some(self.chain.safe_block_hash),
                alloy_eips::BlockNumberOrTag::Finalized => Some(self.chain.finalized_block_hash),
                alloy_eips::BlockNumberOrTag::Earliest => self.get_block_hash(0),
            },
        }
    }

    pub fn add_payload(&mut self, payload_id: PayloadId, block: Block<Transaction>, receipts: Vec<Receipt>) {
        self.chain.payloads.insert(payload_id, (block, receipts));
    }

    pub fn get_payload(&self, payload_id: &PayloadId) -> Option<&(Block<Transaction>, Vec<Receipt>)> {
        self.chain.payloads.get(payload_id)
    }

    pub fn remove_payload(&mut self, payload_id: &PayloadId) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        self.chain.payloads.remove(payload_id)
    }

    pub fn update_forkchoice(&mut self, head: B256, safe: Option<B256>, finalized: Option<B256>) {
        debug!("[Storage] Updating forkchoice: head={:?}, safe={:?}, finalized={:?}", head, safe, finalized);
        FORK_CHOICE_UPDATED_TOTAL.inc();
        
        if self.chain.head_block_hash != head && self.chain.head_block_hash != B256::ZERO {
            if let Some(new_block) = self.get_block_by_hash(head) {
                if new_block.header.parent_hash != self.chain.head_block_hash {
                    REORG_COUNT_TOTAL.inc();
                    info!("[Storage] Reorg detected! Old head: {:?}, New head: {:?}, New parent: {:?}", self.chain.head_block_hash, head, new_block.header.parent_hash);
                }
            }
        }

        self.chain.head_block_hash = head;
        if let Some(s) = safe {
            self.chain.safe_block_hash = s;
        }
        if let Some(f) = finalized {
            self.chain.finalized_block_hash = f;
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

impl SyncStateProvider for StateStore {
    fn set_block_environment(&mut self, number: u64, timestamp: u64, base_fee: U256) {
        self.backend.environment.block_number = EvmU256::from(number);
        self.backend.environment.block_timestamp = EvmU256::from(timestamp);
        self.backend.environment.block_base_fee_per_gas = alloy_u256_to_evm_u256(base_fee);
    }

    fn get_account(&self, address: Address) -> Option<InMemoryAccount> {
        self.backend.state.get(&address_to_h160(address)).cloned()
    }

    fn apply_changeset(&mut self, changeset: &OverlayedChangeSet) {
        self.backend.apply_overlayed(changeset);
    }

    fn backend(&self) -> &InMemoryBackend {
        &self.backend
    }

    fn add_transaction(&mut self, _tx: Transaction) {}
    fn add_receipt(&mut self, _tx_hash: B256, _receipt: Receipt) {}
    fn add_block(&mut self, _block: Block<Transaction>) {}

    fn set_account(&mut self, address: Address, account: InMemoryAccount) {
        self.backend.state.insert(address_to_h160(address), account);
    }

    fn calculate_state_root(&self) -> B256 {
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
                balance: evm_u256_to_alloy_u256(acc.balance),
                storage_root,
                code_hash: alloy_primitives::keccak256(&acc.code),
            };
            (Address::from(addr.0), trie_acc)
        }))
    }

    fn clone_box(&self) -> Box<dyn SyncStateProvider> {
        Box::new(self.clone())
    }
}

impl SyncStateProvider for InMemoryStorage {
    fn set_block_environment(&mut self, number: u64, timestamp: u64, base_fee: U256) {
        self.state.set_block_environment(number, timestamp, base_fee);
    }

    fn get_account(&self, address: Address) -> Option<InMemoryAccount> {
        self.state.get_account(address)
    }

    fn apply_changeset(&mut self, changeset: &OverlayedChangeSet) {
        self.state.apply_changeset(changeset);
    }

    fn backend(&self) -> &InMemoryBackend {
        self.state.backend()
    }

    fn add_transaction(&mut self, tx: Transaction) {
        self.add_transaction(tx);
    }

    fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        self.add_receipt(tx_hash, receipt);
    }

    fn add_block(&mut self, block: Block<Transaction>) {
        self.add_block(block);
    }

    fn set_account(&mut self, address: Address, account: InMemoryAccount) {
        self.state.set_account(address, account);
    }

    fn calculate_state_root(&self) -> B256 {
        self.state.calculate_state_root()
    }

    fn clone_box(&self) -> Box<dyn SyncStateProvider> {
        Box::new(self.clone())
    }
}

#[async_trait]
impl StateSnapshot for InMemoryStorage {
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[async_trait]
impl StateProvider for StateStore {
    fn writer(&self) -> Arc<dyn WriteProvider> { unimplemented!() }
    async fn account(&self, address: Address, _id: BlockId) -> Result<Option<GenesisAccount>> {
        let h160 = address_to_h160(address);
        Ok(self.backend.state.get(&h160).map(|acc| GenesisAccount {
            balance: evm_u256_to_alloy_u256(acc.balance),
            nonce: Some(acc.nonce.as_u64()),
            code: if acc.code.is_empty() { None } else { Some(acc.code.clone().into()) },
            storage: if acc.storage.is_empty() { None } else {
                Some(acc.storage.iter().map(|(k, v)| (B256::from(k.0), B256::from(v.0))).collect())
            },
            private_key: None,
        }))
    }
    async fn storage(&self, address: Address, slot: B256, _id: BlockId) -> Result<Option<U256>> {
        let h160 = address_to_h160(address);
        Ok(self.backend.state.get(&h160).and_then(|a| a.storage.get(&H256(slot.0)).map(|v| U256::from_be_bytes(v.0))))
    }
    async fn code(&self, address: Address, _id: BlockId) -> Result<Option<Bytes>> {
        let h160 = address_to_h160(address);
        Ok(self.backend.state.get(&h160).map(|a| a.code.clone().into()))
    }
    async fn balance(&self, address: Address, _id: BlockId) -> Result<U256> {
        let h160 = address_to_h160(address);
        Ok(self.backend.state.get(&h160).map(|a| evm_u256_to_alloy_u256(a.balance)).unwrap_or(U256::ZERO))
    }
    async fn transaction_count(&self, address: Address, _id: BlockId) -> Result<u64> {
        let h160 = address_to_h160(address);
        Ok(self.backend.state.get(&h160).map(|a| a.nonce.as_u64()).unwrap_or(0))
    }
    async fn accounts(&self) -> Result<Vec<Address>> {
        Ok(self.backend.state.keys().map(|h| Address::from(h.0)).collect())
    }
    async fn header(&self, _id: BlockId) -> Result<Option<Header>> { Ok(None) }
    async fn block(&self, _id: BlockId) -> Result<Option<Block<Transaction>>> { Ok(None) }
    async fn block_hash(&self, _n: u64) -> Result<Option<B256>> { Ok(None) }
    async fn latest_block_number(&self) -> Result<u64> { Ok(0) }
    async fn chain_id(&self) -> Result<u64> { Ok(evm_u256_to_alloy_u256(self.backend.environment.chain_id).to::<u64>()) }
    async fn logs(&self, _f: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::eth::Log>> { Ok(vec![]) }
    async fn transaction(&self, _h: B256) -> Result<Option<Transaction>> { Ok(None) }
    async fn transaction_receipt(&self, _h: B256) -> Result<Option<Receipt>> { Ok(None) }
    async fn transaction_block_reference(&self, _h: B256) -> Result<Option<(u64, B256, usize)>> { Ok(None) }
    async fn get_snapshot(&self) -> Result<Box<dyn StateSnapshot>> { unimplemented!() }
}

#[async_trait]
impl StateProvider for ChainStore {
    fn writer(&self) -> Arc<dyn WriteProvider> { unimplemented!() }
    async fn account(&self, _a: Address, _id: BlockId) -> Result<Option<GenesisAccount>> { Ok(None) }
    async fn storage(&self, _a: Address, _s: B256, _id: BlockId) -> Result<Option<U256>> { Ok(None) }
    async fn code(&self, _a: Address, _id: BlockId) -> Result<Option<Bytes>> { Ok(None) }
    async fn balance(&self, _a: Address, _id: BlockId) -> Result<U256> { Ok(U256::ZERO) }
    async fn transaction_count(&self, _a: Address, _id: BlockId) -> Result<u64> { Ok(0) }
    async fn accounts(&self) -> Result<Vec<Address>> { Ok(vec![]) }
    async fn header(&self, id: BlockId) -> Result<Option<Header>> { Ok(self.get_block_by_id(id).map(|b| b.header.clone())) }
    async fn block(&self, id: BlockId) -> Result<Option<Block<Transaction>>> { Ok(self.get_block_by_id(id).cloned()) }
    async fn block_hash(&self, n: u64) -> Result<Option<B256>> { Ok(self.blocks.get(&n).map(|b| b.header.hash_slow())) }
    async fn latest_block_number(&self) -> Result<u64> { Ok(self.blocks.keys().last().cloned().unwrap_or(0)) }
    async fn chain_id(&self) -> Result<u64> { Ok(0) }
    async fn logs(&self, _f: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::eth::Log>> { Ok(vec![]) }
    async fn transaction(&self, h: B256) -> Result<Option<Transaction>> { Ok(self.transactions.get(&h).cloned()) }
    async fn transaction_receipt(&self, h: B256) -> Result<Option<Receipt>> { Ok(self.receipts.get(&h).cloned()) }
    async fn transaction_block_reference(&self, h: B256) -> Result<Option<(u64, B256, usize)>> { Ok(self.tx_location.get(&h).cloned()) }
    async fn get_snapshot(&self) -> Result<Box<dyn StateSnapshot>> { unimplemented!() }
}

impl ChainStore {
    pub fn get_block_by_id(&self, block_id: BlockId) -> Option<&Block<Transaction>> {
        match block_id {
            BlockId::Hash(hash) => {
                let h: B256 = hash.into();
                self.hash_to_number.get(&h).and_then(|&num| self.blocks.get(&num))
            }
            BlockId::Number(num) => match num {
                alloy_eips::BlockNumberOrTag::Number(n) => self.blocks.get(&n),
                alloy_eips::BlockNumberOrTag::Latest | alloy_eips::BlockNumberOrTag::Pending => {
                    self.hash_to_number.get(&self.head_block_hash).and_then(|&num| self.blocks.get(&num))
                }
                alloy_eips::BlockNumberOrTag::Safe => self.hash_to_number.get(&self.safe_block_hash).and_then(|&num| self.blocks.get(&num)),
                alloy_eips::BlockNumberOrTag::Finalized => self.hash_to_number.get(&self.finalized_block_hash).and_then(|&num| self.blocks.get(&num)),
                alloy_eips::BlockNumberOrTag::Earliest => self.blocks.get(&0),
            }
        }
    }
}

#[async_trait]
impl StateProvider for InMemoryStorage {
    fn writer(&self) -> Arc<dyn WriteProvider> { unimplemented!() }
    async fn account(&self, address: Address, block_id: BlockId) -> Result<Option<GenesisAccount>> { self.state.account(address, block_id).await }
    async fn storage(&self, address: Address, slot: B256, block_id: BlockId) -> Result<Option<U256>> { self.state.storage(address, slot, block_id).await }
    async fn code(&self, address: Address, block_id: BlockId) -> Result<Option<Bytes>> { self.state.code(address, block_id).await }
    async fn balance(&self, address: Address, block_id: BlockId) -> Result<U256> { self.state.balance(address, block_id).await }
    async fn transaction_count(&self, address: Address, block_id: BlockId) -> Result<u64> { self.state.transaction_count(address, block_id).await }
    async fn accounts(&self) -> Result<Vec<Address>> { self.state.accounts().await }
    async fn header(&self, block_id: BlockId) -> Result<Option<Header>> { self.chain.header(block_id).await }
    async fn block(&self, block_id: BlockId) -> Result<Option<Block<Transaction>>> { self.chain.block(block_id).await }
    async fn block_hash(&self, number: u64) -> Result<Option<B256>> { self.chain.block_hash(number).await }
    async fn latest_block_number(&self) -> Result<u64> { self.chain.latest_block_number().await }
    async fn chain_id(&self) -> Result<u64> { self.state.chain_id().await }
    async fn logs(&self, filter: alloy_rpc_types::Filter) -> Result<Vec<alloy_rpc_types::eth::Log>> { self.chain.logs(filter).await }
    async fn transaction(&self, hash: B256) -> Result<Option<Transaction>> { self.chain.transaction(hash).await }
    async fn transaction_receipt(&self, hash: B256) -> Result<Option<Receipt>> { self.chain.transaction_receipt(hash).await }
    async fn transaction_block_reference(&self, hash: B256) -> Result<Option<(u64, B256, usize)>> { self.chain.transaction_block_reference(hash).await }
    async fn get_snapshot(&self) -> Result<Box<dyn StateSnapshot>> { Ok(Box::new(self.clone())) }
}

impl Database for InMemoryStorage {
    fn state_store(&self) -> Arc<dyn StateProvider> { Arc::new(self.state.clone()) }
    fn chain_store(&self) -> Arc<dyn StateProvider> { Arc::new(self.chain.clone()) }
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
        assert_eq!(storage.state.backend.environment.chain_id, chain_id);
        assert_eq!(storage.get_latest_block_number(), 0);
    }

    #[tokio::test]
    async fn test_add_block() {
        let mut storage = InMemoryStorage::new(EvmU256::from(1));
        let genesis_hash = storage.chain.head_block_hash;
        
        let mut block: Block<Transaction> = Block::default();
        block.header.number = 1;
        block.header.parent_hash = genesis_hash;
        
        storage.add_block(block);
        assert_eq!(storage.get_latest_block_number(), 1);
        assert_ne!(storage.chain.head_block_hash, genesis_hash);
    }

    #[tokio::test]
    async fn test_forkchoice_update() {
        let mut storage = InMemoryStorage::new(EvmU256::from(1));
        let genesis_hash = storage.chain.head_block_hash;
        let new_hash = B256::repeat_byte(0x12);
        
        storage.update_forkchoice(new_hash, Some(genesis_hash), Some(genesis_hash));
        assert_eq!(storage.chain.head_block_hash, new_hash);
        assert_eq!(storage.chain.safe_block_hash, genesis_hash);
        assert_eq!(storage.chain.finalized_block_hash, genesis_hash);
    }
}
