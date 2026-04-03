use crate::ev::{H160, EvmU256, evm};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, InMemoryAccount};
use alloy_primitives::{Address, B256, U256};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Transaction {
    pub hash: B256,
    #[allow(dead_code)]
    pub nonce: u64,
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: U256,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub number: u64,
    #[allow(dead_code)]
    pub hash: B256,
    #[allow(dead_code)]
    pub parent_hash: B256,
    pub timestamp: u64,
    #[allow(dead_code)]
    pub transactions: Vec<B256>,
}

pub struct InMemoryStorage {
    pub backend: InMemoryBackend,
    pub blocks: BTreeMap<u64, Block>,
    pub transactions: BTreeMap<B256, Transaction>,
    #[allow(dead_code)]
    pub contracts: BTreeMap<Address, Vec<u8>>,
}

impl InMemoryStorage {
    pub fn new(chain_id: EvmU256) -> Self {
        let env = InMemoryEnvironment {
            block_hashes: BTreeMap::new(),
            block_number: EvmU256::zero(),
            block_coinbase: H160::zero(),
            block_timestamp: EvmU256::zero(),
            block_difficulty: EvmU256::zero(),
            block_randomness: None,
            block_gas_limit: EvmU256::max_value(),
            block_base_fee_per_gas: EvmU256::zero(),
            blob_base_fee_per_gas: EvmU256::zero(),
            blob_versioned_hashes: Vec::new(),
            chain_id,
        };

        Self {
            backend: InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            },
            blocks: BTreeMap::new(),
            transactions: BTreeMap::new(),
            contracts: BTreeMap::new(),
        }
    }

    pub fn add_block(&mut self, block: Block) {
        self.blocks.insert(block.number, block);
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        self.transactions.insert(tx.hash, tx);
    }

    #[allow(dead_code)]
    pub fn set_contract_code(&mut self, address: Address, code: Vec<u8>) {
        self.contracts.insert(address, code.clone());
        self.backend.state.entry(H160::from_slice(address.as_slice())).or_insert(InMemoryAccount {
            balance: EvmU256::zero(),
            code: code.clone(),
            nonce: EvmU256::zero(),
            storage: BTreeMap::new(),
            transient_storage: BTreeMap::new(),
        }).code = code.clone();
    }

    pub fn set_balance(&mut self, address: Address, balance: U256) {
        let evm_balance = EvmU256::from_big_endian(&balance.to_be_bytes::<32>());
        self.backend.state.entry(H160::from_slice(address.as_slice())).or_insert(InMemoryAccount {
            balance: evm_balance,
            code: Vec::new(),
            nonce: EvmU256::zero(),
            storage: BTreeMap::new(),
            transient_storage: BTreeMap::new(),
        }).balance = evm_balance;
    }
}
