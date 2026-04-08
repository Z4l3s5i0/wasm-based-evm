use crate::ev::{H160, H256, EvmU256, evm, address_to_h160, alloy_u256_to_evm_u256};
use crate::{debug};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, InMemoryAccount};
use alloy_primitives::{Address, B256, U256, keccak256};
use alloy_trie::{TrieAccount, root::ordered_trie_root};
use alloy_trie::root::{state_root_unhashed, storage_root_unsorted};
use std::collections::BTreeMap;
use crate::storage::genesis::Genesis;
use crate::storage::types::{Block, Receipt, Transaction, Withdrawal};


#[derive(Clone)]
pub struct InMemoryStorage {
    pub backend: InMemoryBackend,
    pub blocks: BTreeMap<u64, Block>,
    pub transactions: BTreeMap<B256, Transaction>,
    pub receipts: BTreeMap<B256, Receipt>,
    #[allow(dead_code)]
    pub contracts: BTreeMap<Address, Vec<u8>>,
    pub mempool: crate::mempool::Mempool,
    pub head_block_hash: B256,
    pub safe_block_hash: B256,
    pub finalized_block_hash: B256,
}

impl InMemoryStorage {
    pub fn new(chain_id: EvmU256) -> Self {
        Self::new_with_genesis(chain_id, Genesis::default())
    }

    pub fn new_with_genesis(chain_id: EvmU256, genesis: Genesis) -> Self {
        let env = InMemoryEnvironment {
            block_hashes: BTreeMap::new(),
            block_number: EvmU256::zero(),
            block_coinbase: H160::zero(),
            block_timestamp: EvmU256::from(genesis.timestamp),
            block_difficulty: EvmU256::zero(),
            block_randomness: None,
            block_gas_limit: EvmU256::from(genesis.gas_limit),
            block_base_fee_per_gas: EvmU256::zero(),
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
            transactions: BTreeMap::new(),
            receipts: BTreeMap::new(),
            contracts: BTreeMap::new(),
            mempool: crate::mempool::Mempool::new(U256::ZERO),
            head_block_hash: B256::ZERO,
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
        };

        // Create genesis block
        let genesis_block_builder = Block::builder(0)
            .timestamp(genesis.timestamp)
            .gas_limit(genesis.gas_limit);

        // Pre-fund and initialize accounts
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
            storage.backend.state.insert(address_to_h160(account.address), im_account);
        }

        let state_root = storage.calculate_state_root();
        let genesis_block = genesis_block_builder.state_root(state_root).build();
        let genesis_hash = genesis_block.body.execution_payload.block_hash;
        storage.add_block(genesis_block);
        storage.head_block_hash = genesis_hash;
        storage.safe_block_hash = genesis_hash;
        storage.finalized_block_hash = genesis_hash;

        storage
    }

    pub fn add_block(&mut self, block: Block) {
        let block_number = block.body.execution_payload.block_number;
        let block_hash = block.body.execution_payload.block_hash;
        debug!("[Storage] Adding block #{} with hash {:?}", block_number, block_hash);
        self.blocks.insert(block_number, block);
        self.head_block_hash = block_hash;
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        debug!("[Storage] Adding transaction {:?}", tx.hash);
        self.transactions.insert(tx.hash, tx);
    }

    pub fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        debug!("[Storage] Adding receipt for transaction {:?}", tx_hash);
        self.receipts.insert(tx_hash, receipt);
    }

    pub fn get_receipt_by_tx_hash(&self, tx_hash: B256) -> Option<&Receipt> {
        self.receipts.get(&tx_hash)
    }

    pub fn get_block_by_number(&self, number: u64) -> Option<&Block> {
        self.blocks.get(&number)
    }

    pub fn get_block_by_hash(&self, hash: B256) -> Option<&Block> {
        self.blocks.values().find(|b| b.body.execution_payload.block_hash == hash)
    }

    pub fn get_transaction_by_hash(&self, hash: B256) -> Option<&Transaction> {
        self.transactions.get(&hash)
    }

    pub fn get_block_by_transaction_hash(&self, tx_hash: B256) -> Option<&Block> {
        self.blocks.values().find(|b| b.body.execution_payload.transactions.iter().any(|tx| tx.hash == tx_hash))
    }

    pub fn get_block_receipts(&self, block_hash: B256) -> Vec<Receipt> {
        let block = match self.get_block_by_hash(block_hash) {
            Some(b) => b,
            None => return Vec::new(),
        };
        block.body.execution_payload.transactions.iter()
            .filter_map(|tx| self.receipts.get(&tx.hash).cloned())
            .collect()
    }

    pub fn get_latest_block(&self) -> Option<&Block> {
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

    #[allow(dead_code)]
    pub fn set_contract_code(&mut self, address: Address, code: Vec<u8>) {
        debug!("[Storage] Setting contract code for address {:?}", address);
        self.contracts.insert(address, code.clone());
        self.backend.state.entry(H160::from_slice(address.as_slice())).or_insert(InMemoryAccount {
            balance: EvmU256::zero(),
            code: code.clone(),
            nonce: EvmU256::zero(),
            storage: BTreeMap::<H256, H256>::new(),
            transient_storage: BTreeMap::<H256, H256>::new(),
        }).code = code.clone();
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
            let storage_root = storage_root_unsorted(acc.storage.iter().map(|(k, v)| (B256::from(k.0), U256::from_be_bytes(v.0))));

            let trie_acc = TrieAccount {
                nonce: acc.nonce.as_u64(),
                balance: {
                    let mut b = [0u8; 32];
                    acc.balance.to_big_endian(&mut b);
                    U256::from_be_bytes(b)
                },
                storage_root,
                code_hash: keccak256(&acc.code),
            };
            (Address::from(addr.0), trie_acc)
        }))
    }
}
