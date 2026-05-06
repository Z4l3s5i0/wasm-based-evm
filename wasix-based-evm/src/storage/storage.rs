use crate::evm::ev::{address_to_h160, alloy_u256_to_evm_u256, b256_to_h256, evm, evm_u256_to_alloy_u256, EvmU256, H160, H256};
use crate::evm::executor::Executor;
use crate::mempool::Mempool;
use crate::misc::metrics::{FORK_CHOICE_UPDATED_TOTAL, REORG_COUNT_TOTAL, STORAGE_READ_LATENCY, STORAGE_WRITE_LATENCY};
use crate::storage::traits::{ChainProvider, StateProvider};
use crate::{debug, error, info};
use alloy_consensus::{Block, Header, ReceiptWithBloom as Receipt, TxEnvelope as Transaction};
use alloy_eips::BlockId;
use alloy_genesis::{ChainConfig, Genesis, GenesisAccount};
use alloy_primitives::{Address, Bytes, B256, B64, U256};
use alloy_rpc_types::engine::PayloadId;
use alloy_trie::root::{state_root_unhashed, storage_root_unsorted};
use alloy_trie::TrieAccount;
use anyhow::Result;
use async_trait::async_trait;
use evm::backend::{InMemoryAccount, InMemoryBackend, InMemoryEnvironment};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

const BLOCKS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("blocks");
const HASH_TO_NUMBER_TABLE: TableDefinition<&[u8], u64> = TableDefinition::new("hash_to_number");
const TRANSACTIONS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("transactions");
const RECEIPTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("receipts");
const TX_LOCATION_TABLE: TableDefinition<&[u8], (u64, &[u8], u64)> = TableDefinition::new("tx_location");
const STATE_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("state");
const SNAPSHOTS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("snapshots");
const PAYLOADS_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("payloads");
const METADATA_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("metadata");

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

#[derive(Clone)]
pub struct RedbStorage {
    pub db: Arc<Database>,
    pub head_block_hash: B256,
    pub safe_block_hash: B256,
    pub finalized_block_hash: B256,
    pub backend: InMemoryBackend, // Keep the latest backend in memory for fast access
}

impl RedbStorage {
    fn create_db<P: AsRef<Path>>(path: P) -> Arc<Database> {
        let db = Database::builder()
                .create(path)
                .expect("Failed to create file-backed database");
        
        // Initialize tables
        let write_txn = db.begin_write().unwrap();
        {
            write_txn.open_table(BLOCKS_TABLE).unwrap();
            write_txn.open_table(HASH_TO_NUMBER_TABLE).unwrap();
            write_txn.open_table(TRANSACTIONS_TABLE).unwrap();
            write_txn.open_table(RECEIPTS_TABLE).unwrap();
            write_txn.open_table(TX_LOCATION_TABLE).unwrap();
            write_txn.open_table(STATE_TABLE).unwrap();
            write_txn.open_table(SNAPSHOTS_TABLE).unwrap();
            write_txn.open_table(PAYLOADS_TABLE).unwrap();
            write_txn.open_table(METADATA_TABLE).unwrap();
        }
        write_txn.commit().unwrap();
        Arc::new(db)
    }

    pub fn new<P: AsRef<Path>>(chain_id: EvmU256, path: P) -> Self {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = evm_u256_to_alloy_u256(chain_id).to::<u64>();
        Self::new_with_genesis(chain_id, genesis, path)
    }

    pub fn new_with_genesis<P: AsRef<Path>>(chain_id: EvmU256, genesis: Genesis, path: P) -> Self {
        let (mut storage, genesis_block) = Self::create_from_genesis(chain_id, genesis, path);
        let genesis_hash = genesis_block.header.hash_slow();
        
        storage.add_block(genesis_block);
        storage.update_forkchoice(genesis_hash, Some(genesis_hash), Some(genesis_hash));

        storage
    }

    pub fn new_with_genesis_init<P: AsRef<Path>>(chain_id: EvmU256, genesis: GenesisInit, path: P) -> Self {
        let (mut storage, genesis_block) = Self::create_from_genesis_init(chain_id, genesis, path);
        let genesis_hash = genesis_block.header.hash_slow();
        
        storage.add_block(genesis_block);
        storage.update_forkchoice(genesis_hash, Some(genesis_hash), Some(genesis_hash));

        storage
    }

    pub fn create_from_genesis_init<P: AsRef<Path>>(chain_id: EvmU256, genesis: GenesisInit, path: P) -> (Self, Block<Transaction>) {
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
            db: Self::create_db(path),
            backend: InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            },
            head_block_hash: B256::ZERO,
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
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

    pub fn create_from_genesis<P: AsRef<Path>>(chain_id: EvmU256, genesis: Genesis, path: P) -> (Self, Block<Transaction>) {
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
            db: Self::create_db(path),
            backend: InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            },
            head_block_hash: B256::ZERO,
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
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

    pub fn new_with_genesis_block(_chain_id: EvmU256, mut storage: Self, genesis_block: Block<Transaction>) -> Self {
        let genesis_hash = genesis_block.header.hash_slow();
        
        storage.add_block(genesis_block);
        storage.update_forkchoice(genesis_hash, Some(genesis_hash), Some(genesis_hash));

        storage
    }

    pub fn add_block(&mut self, block: Block<Transaction>) {
        let start = Instant::now();
        let block_number = block.header.number;
        let block_hash = block.header.hash_slow();
        
        // Check if we already have this block
        if self.get_block_by_hash(block_hash).is_some() {
            debug!("[Storage] Block #{} with hash {:?} already exists, skipping", block_number, block_hash);
            return;
        }

        info!("[Storage] Adding block #{} with hash {:?}", block_number, block_hash);
        
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut blocks_table = write_txn.open_table(BLOCKS_TABLE).unwrap();
            let mut hash_to_number_table = write_txn.open_table(HASH_TO_NUMBER_TABLE).unwrap();
            let mut transactions_table = write_txn.open_table(TRANSACTIONS_TABLE).unwrap();
            let mut tx_location_table = write_txn.open_table(TX_LOCATION_TABLE).unwrap();
            let mut snapshots_table = write_txn.open_table(SNAPSHOTS_TABLE).unwrap();

            for (i, tx) in block.body.transactions.iter().enumerate() {
                let tx_hash = tx.hash();
                debug!("[Storage] Indexing transaction {:?} in block {}", tx_hash, block_number);
                tx_location_table.insert(tx_hash.as_slice(), (block_number, block_hash.as_slice(), i as u64)).unwrap();
                let tx_bytes = serde_json::to_vec(tx).unwrap();
                transactions_table.insert(tx_hash.as_slice(), tx_bytes.as_slice()).unwrap();
            }

            let block_bytes = serde_json::to_vec(&block).unwrap();
            blocks_table.insert(block_number, block_bytes.as_slice()).unwrap();
            hash_to_number_table.insert(block_hash.as_slice(), block_number).unwrap();
            
            // Take snapshot after adding block
            let snapshot_bytes = serde_json::to_vec(&self.backend).unwrap();
            snapshots_table.insert(block_number, snapshot_bytes.as_slice()).unwrap();
            
            // Keep only last 100 snapshots
            // This is a bit inefficient with redb, but let's stick to the logic for now
            let mut count = 0;
            let mut first_key = None;
            for item in snapshots_table.iter().unwrap() {
                let (key, _) = item.unwrap();
                if count == 0 {
                    first_key = Some(key.value());
                }
                count += 1;
            }
            if count > 100 {
                if let Some(key) = first_key {
                    snapshots_table.remove(key).unwrap();
                }
            }
        }
        write_txn.commit().unwrap();
        
        self.head_block_hash = block_hash;
        STORAGE_WRITE_LATENCY.observe(start.elapsed().as_secs_f64());
    }

    pub fn revert_to_height(&mut self, height: u64) -> Vec<Transaction> {
        info!("[Storage] Reverting to height {}", height);
        let mut reverted_txs = Vec::new();
        
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut blocks_table = write_txn.open_table(BLOCKS_TABLE).unwrap();
            let mut hash_to_number_table = write_txn.open_table(HASH_TO_NUMBER_TABLE).unwrap();
            let mut tx_location_table = write_txn.open_table(TX_LOCATION_TABLE).unwrap();
            let snapshots_table = write_txn.open_table(SNAPSHOTS_TABLE).unwrap();

            let keys_to_remove: Vec<u64> = blocks_table.range((height + 1)..).unwrap()
                .map(|item| item.unwrap().0.value()).collect();

            for k in keys_to_remove {
                if let Some(block_bytes) = blocks_table.remove(k).unwrap() {
                    let block: Block<Transaction> = serde_json::from_slice(block_bytes.value()).unwrap();
                    let block_hash = block.header.hash_slow();
                    hash_to_number_table.remove(block_hash.as_slice()).unwrap();
                    for tx in block.body.transactions {
                        let hash = tx.hash();
                        tx_location_table.remove(hash.as_slice()).unwrap();
                        reverted_txs.push(tx);
                    }
                }
            }
            
            if let Some(snapshot_bytes) = snapshots_table.get(height).unwrap() {
                self.backend = serde_json::from_slice(snapshot_bytes.value()).unwrap();
            } else {
                error!("[Storage] No snapshot found for height {}, state might be inconsistent!", height);
            }
            
            if let Some(block_bytes) = blocks_table.get(height).unwrap() {
                let block: Block<Transaction> = serde_json::from_slice(block_bytes.value()).unwrap();
                self.head_block_hash = block.header.hash_slow();
            }
        }
        write_txn.commit().unwrap();
        
        reverted_txs
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        let hash = tx.hash();
        debug!("[Storage] Adding transaction {:?}", hash);
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut transactions_table = write_txn.open_table(TRANSACTIONS_TABLE).unwrap();
            let tx_bytes = serde_json::to_vec(&tx).unwrap();
            transactions_table.insert(hash.as_slice(), tx_bytes.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }

    pub fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        debug!("[Storage] Adding receipt for transaction {:?}", tx_hash);
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut receipts_table = write_txn.open_table(RECEIPTS_TABLE).unwrap();
            let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
            receipts_table.insert(tx_hash.as_slice(), receipt_bytes.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }

    pub fn get_receipt_by_tx_hash(&self, tx_hash: B256) -> Option<Receipt> {
        let read_txn = self.db.begin_read().unwrap();
        let receipts_table = read_txn.open_table(RECEIPTS_TABLE).unwrap();
        receipts_table.get(tx_hash.as_slice()).unwrap().map(|r| serde_json::from_slice(r.value()).unwrap())
    }

    pub fn get_block_by_number(&self, number: u64) -> Option<Block<Transaction>> {
        let read_txn = self.db.begin_read().unwrap();
        let blocks_table = read_txn.open_table(BLOCKS_TABLE).unwrap();
        blocks_table.get(number).unwrap().map(|b| serde_json::from_slice(b.value()).unwrap())
    }

    pub fn get_block_by_hash(&self, hash: B256) -> Option<Block<Transaction>> {
        let read_txn = self.db.begin_read().unwrap();
        let hash_to_number_table = read_txn.open_table(HASH_TO_NUMBER_TABLE).unwrap();
        let blocks_table = read_txn.open_table(BLOCKS_TABLE).unwrap();
        hash_to_number_table.get(hash.as_slice()).unwrap()
            .and_then(|num| blocks_table.get(num.value()).unwrap())
            .map(|b| serde_json::from_slice(b.value()).unwrap())
    }

    pub fn get_transaction_by_hash(&self, hash: B256) -> Option<Transaction> {
        let read_txn = self.db.begin_read().unwrap();
        let transactions_table = read_txn.open_table(TRANSACTIONS_TABLE).unwrap();
        transactions_table.get(hash.as_slice()).unwrap().map(|tx| serde_json::from_slice(tx.value()).unwrap())
    }

    pub fn get_block_by_transaction_hash(&self, tx_hash: B256) -> Option<Block<Transaction>> {
        let read_txn = self.db.begin_read().unwrap();
        let tx_location_table = read_txn.open_table(TX_LOCATION_TABLE).unwrap();
        tx_location_table.get(tx_hash.as_slice()).unwrap().and_then(|loc| {
            let (num, _, _) = loc.value();
            self.get_block_by_number(num)
        })
    }

    pub fn get_block_receipts(&self, block_hash: B256) -> Vec<Receipt> {
        let block = match self.get_block_by_hash(block_hash) {
            Some(b) => b,
            None => return Vec::new(),
        };
        let read_txn = self.db.begin_read().unwrap();
        let receipts_table = read_txn.open_table(RECEIPTS_TABLE).unwrap();
        block.body.transactions.iter()
            .filter_map(|tx| {
                receipts_table.get(tx.hash().as_slice()).unwrap()
                    .map(|r| serde_json::from_slice(r.value()).unwrap())
            })
            .collect()
    }

    pub fn get_latest_block(&self) -> Option<Block<Transaction>> {
        let read_txn = self.db.begin_read().unwrap();
        let blocks_table = read_txn.open_table(BLOCKS_TABLE).unwrap();
        blocks_table.iter().unwrap().last().map(|item| serde_json::from_slice(item.unwrap().1.value()).unwrap())
    }

    pub fn get_latest_block_number(&self) -> u64 {
        let read_txn = self.db.begin_read().unwrap();
        let blocks_table = read_txn.open_table(BLOCKS_TABLE).unwrap();
        blocks_table.iter().unwrap().last().map(|item| item.unwrap().0.value()).unwrap_or(0)
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

    pub fn get_block_by_id(&self, block_id: BlockId) -> Option<Block<Transaction>> {
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
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut payloads_table = write_txn.open_table(PAYLOADS_TABLE).unwrap();
            let payload_bytes = serde_json::to_vec(&(block, receipts)).unwrap();
            let bytes: [u8; 8] = payload_id.0.to_vec().try_into().unwrap();
            let id_u64 = u64::from_be_bytes(bytes);
            payloads_table.insert(id_u64, payload_bytes.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }

    pub fn get_payload(&self, payload_id: &PayloadId) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        let read_txn = self.db.begin_read().unwrap();
        let payloads_table = read_txn.open_table(PAYLOADS_TABLE).unwrap();
        let bytes: [u8; 8] = payload_id.0.to_vec().try_into().unwrap();
        let id_u64 = u64::from_be_bytes(bytes);
        payloads_table.get(id_u64).unwrap().map(|p| serde_json::from_slice(p.value()).unwrap())
    }

    pub fn remove_payload(&mut self, payload_id: &PayloadId) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        let write_txn = self.db.begin_write().unwrap();
        let mut res = None;
        {
            let mut payloads_table = write_txn.open_table(PAYLOADS_TABLE).unwrap();
            let bytes: [u8; 8] = payload_id.0.to_vec().try_into().unwrap();
            let id_u64 = u64::from_be_bytes(bytes);
            if let Some(p) = payloads_table.remove(id_u64).unwrap() {
                res = Some(serde_json::from_slice(p.value()).unwrap());
            }
        }
        write_txn.commit().unwrap();
        res
    }

    pub fn get_payload_by_block_hash(&self, hash: B256) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        let read_txn = self.db.begin_read().unwrap();
        let payloads_table = read_txn.open_table(PAYLOADS_TABLE).unwrap();
        for item in payloads_table.iter().unwrap() {
            let (_, value) = item.unwrap();
            let (block, receipts): (Block<Transaction>, Vec<Receipt>) = serde_json::from_slice(value.value()).unwrap();
            if block.header.hash_slow() == hash {
                return Some((block, receipts));
            }
        }
        None
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

        // Persist metadata
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut metadata_table = write_txn.open_table(METADATA_TABLE).unwrap();
            metadata_table.insert("head_block_hash", head.as_slice()).unwrap();
            if let Some(s) = safe {
                metadata_table.insert("safe_block_hash", s.as_slice()).unwrap();
            }
            if let Some(f) = finalized {
                metadata_table.insert("finalized_block_hash", f.as_slice()).unwrap();
            }
            
            let env_bytes = serde_json::to_vec(&self.backend.environment).unwrap();
            metadata_table.insert("environment", env_bytes.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }
    
    pub fn save_to_vec_pretty(&self) -> Result<Vec<u8>> {
        // Since we can't easily serialize the entire redb Database, 
        // we export the key components that defined the old InMemoryStorage
        #[derive(Serialize)]
        struct StorageDump {
            head_block_hash: B256,
            safe_block_hash: B256,
            finalized_block_hash: B256,
            backend: InMemoryBackend,
        }
        
        let dump = StorageDump {
            head_block_hash: self.head_block_hash,
            safe_block_hash: self.safe_block_hash,
            finalized_block_hash: self.finalized_block_hash,
            backend: self.backend.clone(),
        };
        
        serde_json::to_vec_pretty(&dump).map_err(|e| anyhow::anyhow!(e))
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::builder()
            .create(path)
            .expect("Failed to open database");
        
        // Ensure tables exist
        let write_txn = db.begin_write().unwrap();
        {
            write_txn.open_table(BLOCKS_TABLE).unwrap();
            write_txn.open_table(HASH_TO_NUMBER_TABLE).unwrap();
            write_txn.open_table(TRANSACTIONS_TABLE).unwrap();
            write_txn.open_table(RECEIPTS_TABLE).unwrap();
            write_txn.open_table(TX_LOCATION_TABLE).unwrap();
            write_txn.open_table(STATE_TABLE).unwrap();
            write_txn.open_table(SNAPSHOTS_TABLE).unwrap();
            write_txn.open_table(PAYLOADS_TABLE).unwrap();
            write_txn.open_table(METADATA_TABLE).unwrap();
        }
        write_txn.commit().unwrap();

        let read_txn = db.begin_read().unwrap();
        let metadata_table = read_txn.open_table(METADATA_TABLE).unwrap();
        
        let head_block_hash = metadata_table.get("head_block_hash").unwrap()
            .map(|v| B256::from_slice(v.value())).unwrap_or(B256::ZERO);
        let safe_block_hash = metadata_table.get("safe_block_hash").unwrap()
            .map(|v| B256::from_slice(v.value())).unwrap_or(B256::ZERO);
        let finalized_block_hash = metadata_table.get("finalized_block_hash").unwrap()
            .map(|v| B256::from_slice(v.value())).unwrap_or(B256::ZERO);
        
        let env = metadata_table.get("environment").unwrap()
            .map(|v| serde_json::from_slice(v.value()).unwrap())
            .unwrap_or_else(|| InMemoryEnvironment {
                block_hashes: BTreeMap::new(),
                block_number: EvmU256::zero(),
                block_coinbase: H160::default(),
                block_timestamp: EvmU256::zero(),
                block_difficulty: EvmU256::zero(),
                block_randomness: None,
                block_gas_limit: EvmU256::zero(),
                block_base_fee_per_gas: EvmU256::zero(),
                blob_base_fee_per_gas: EvmU256::zero(),
                blob_versioned_hashes: vec![],
                chain_id: EvmU256::zero(),
            });

        // Current redb implementation keeps state in the latest backend for compatibility
        // In a real implementation, we should load it from STATE_TABLE
        let snapshots_table = read_txn.open_table(SNAPSHOTS_TABLE).unwrap();
        let latest_block_number = read_txn.open_table(BLOCKS_TABLE).unwrap().iter().unwrap().last()
            .map(|item| item.unwrap().0.value()).unwrap_or(0);
        
        let backend = snapshots_table.get(latest_block_number).unwrap()
            .map(|v| serde_json::from_slice(v.value()).unwrap())
            .unwrap_or(InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            });

        Ok(Self {
            db: Arc::new(db),
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
            backend,
        })
    }
}

pub struct StorageProvider {
    inner: Arc<RwLock<RedbStorage>>,
    mempool: Arc<RwLock<Mempool>>,
    executor: Arc<Executor>,
}

impl StorageProvider {
    pub fn new(
        storage: Arc<RwLock<RedbStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        executor: Arc<Executor>,
    ) -> Self {
        Self {
            inner: storage,
            mempool,
            executor,
        }
    }

    async fn get_pending_state(&self) -> Result<RedbStorage> {
        let storage = self.inner.read().await.clone();
        let transactions = self.mempool.read().await.peek_transactions(100); // Take a reasonable amount for pending
        
        if transactions.is_empty() {
            return Ok(storage);
        }

        let mut pending_storage = storage;
        let latest_block = pending_storage.get_latest_block().unwrap();
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

    async fn transaction_block_reference(&self, hash: B256) -> Result<Option<(u64, B256, u64)>> {
        self.inner.read().await.transaction_block_reference(hash).await
    }
}

#[async_trait]
impl StateProvider for RedbStorage {
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
        Ok(self.get_block_by_id(block_id))
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
        Ok(self.get_transaction_by_hash(hash))
    }

    async fn transaction_receipt(&self, hash: B256) -> Result<Option<Receipt>> {
        Ok(self.get_receipt_by_tx_hash(hash))
    }

    async fn transaction_block_reference(&self, hash: B256) -> Result<Option<(u64, B256, u64)>> {
        let read_txn = self.db.begin_read().unwrap();
        let tx_location_table = read_txn.open_table(TX_LOCATION_TABLE).unwrap();
        Ok(tx_location_table.get(hash.as_slice()).unwrap().map(|loc| {
            let (num, h, i) = loc.value();
            (num, B256::from_slice(h), i)
        }))
    }
}

#[async_trait]
impl ChainProvider for RedbStorage {
    fn add_block(&mut self, block: Block<Transaction>) {
        self.add_block(block)
    }

    fn revert_to_height(&mut self, height: u64) -> Vec<Transaction> {
        self.revert_to_height(height)
    }

    fn add_transaction(&mut self, tx: Transaction) {
        self.add_transaction(tx)
    }

    fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        self.add_receipt(tx_hash, receipt)
    }

    fn calculate_state_root(&self) -> B256 {
        self.calculate_state_root()
    }

    fn add_payload(
        &mut self,
        payload_id: PayloadId,
        block: Block<Transaction>,
        receipts: Vec<Receipt>,
    ) {
        self.add_payload(payload_id, block, receipts)
    }

    fn get_payload(
        &self,
        payload_id: &PayloadId,
    ) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        self.get_payload(payload_id)
    }

    fn remove_payload(
        &mut self,
        payload_id: &PayloadId,
    ) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        self.remove_payload(payload_id)
    }

    fn update_forkchoice(
        &mut self,
        head: B256,
        safe: Option<B256>,
        finalized: Option<B256>,
    ) {
        self.update_forkchoice(head, safe, finalized)
    }
}

#[async_trait]
impl ChainProvider for StorageProvider {
    fn add_block(&mut self, block: Block<Transaction>) {
        let mut inner = self.inner.blocking_write();
        inner.add_block(block);
    }

    fn revert_to_height(&mut self, height: u64) -> Vec<Transaction> {
        let mut inner = self.inner.blocking_write();
        inner.revert_to_height(height)
    }

    fn add_transaction(&mut self, tx: Transaction) {
        let mut inner = self.inner.blocking_write();
        inner.add_transaction(tx);
    }

    fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        let mut inner = self.inner.blocking_write();
        inner.add_receipt(tx_hash, receipt);
    }

    fn calculate_state_root(&self) -> B256 {
        let inner = self.inner.blocking_read();
        inner.calculate_state_root()
    }

    fn add_payload(
        &mut self,
        payload_id: PayloadId,
        block: Block<Transaction>,
        receipts: Vec<Receipt>,
    ) {
        let mut inner = self.inner.blocking_write();
        inner.add_payload(payload_id, block, receipts);
    }

    fn get_payload(
        &self,
        payload_id: &PayloadId,
    ) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        let inner = self.inner.blocking_read();
        inner.get_payload(payload_id)
    }

    fn remove_payload(
        &mut self,
        payload_id: &PayloadId,
    ) -> Option<(Block<Transaction>, Vec<Receipt>)> {
        let mut inner = self.inner.blocking_write();
        inner.remove_payload(payload_id)
    }

    fn update_forkchoice(
        &mut self,
        head: B256,
        safe: Option<B256>,
        finalized: Option<B256>,
    ) {
        let mut inner = self.inner.blocking_write();
        inner.update_forkchoice(head, safe, finalized);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::SignableTransaction;
    use alloy_primitives::address;

    #[tokio::test]
    async fn test_new_storage() {
        let chain_id = EvmU256::from(1);
        let storage = RedbStorage::new(chain_id, None::<PathBuf>);
        assert_eq!(storage.backend.environment.chain_id, chain_id);
        assert_eq!(storage.get_latest_block_number(), 0);
    }

    #[tokio::test]
    async fn test_balance_management() {
        let mut storage = RedbStorage::new(EvmU256::from(1), None::<PathBuf>);
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
        
        let (storage, _block) = RedbStorage::create_from_genesis_init(chain_id, genesis_init, None::<PathBuf>);
        assert_eq!(storage.get_balance(addr), U256::from(1000));
        let acc = storage.account(addr, BlockId::latest()).await.unwrap().unwrap();
        assert_eq!(acc.nonce, Some(1));
    }

    #[tokio::test]
    async fn test_block_management() {
        let mut storage = RedbStorage::new(EvmU256::from(1), None::<PathBuf>);
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
        let mut storage = RedbStorage::new(EvmU256::from(1), None::<PathBuf>);
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
        let mut storage = RedbStorage::new(EvmU256::from(1), None::<PathBuf>);
        let h1 = B256::repeat_byte(1);
        let h2 = B256::repeat_byte(2);
        let h3 = B256::repeat_byte(3);
        
        storage.update_forkchoice(h1, Some(h2), Some(h3));
        assert_eq!(storage.head_block_hash, h1);
        assert_eq!(storage.safe_block_hash, h2);
        assert_eq!(storage.finalized_block_hash, h3);
    }
}

