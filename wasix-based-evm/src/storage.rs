use crate::ev::{H160, EvmU256, evm};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, InMemoryAccount};
use alloy_primitives::{Address, B256, U256};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Transaction {
    pub hash: B256,
    pub nonce: u64,
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: U256,
}

impl Transaction {
    pub fn builder(from: Address) -> TransactionBuilder {
        TransactionBuilder::new(from)
    }
}

pub struct TransactionBuilder {
    hash: Option<B256>,
    nonce: u64,
    from: Address,
    to: Option<Address>,
    value: U256,
    data: Vec<u8>,
    gas_limit: u64,
    gas_price: U256,
}

impl TransactionBuilder {
    pub fn new(from: Address) -> Self {
        Self {
            hash: None,
            nonce: 0,
            from,
            to: None,
            value: U256::ZERO,
            data: Vec::new(),
            gas_limit: 21000,
            gas_price: U256::ZERO,
        }
    }

    pub fn hash(mut self, hash: B256) -> Self {
        self.hash = Some(hash);
        self
    }

    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn to(mut self, to: Option<Address>) -> Self {
        self.to = to;
        self
    }

    pub fn value(mut self, value: U256) -> Self {
        self.value = value;
        self
    }

    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    pub fn gas_price(mut self, gas_price: U256) -> Self {
        self.gas_price = gas_price;
        self
    }

    pub fn build(self) -> Transaction {
        Transaction {
            hash: self.hash.unwrap_or_else(B256::random),
            nonce: self.nonce,
            from: self.from,
            to: self.to,
            value: self.value,
            data: self.data,
            gas_limit: self.gas_limit,
            gas_price: self.gas_price,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub number: u64,
    pub hash: B256,
    pub parent_hash: B256,
    pub timestamp: u64,
    pub transactions: Vec<B256>,
}

impl Block {
    pub fn builder(number: u64) -> BlockBuilder {
        BlockBuilder::new(number)
    }
}

pub struct BlockBuilder {
    number: u64,
    hash: Option<B256>,
    parent_hash: B256,
    timestamp: u64,
    transactions: Vec<B256>,
}

impl BlockBuilder {
    pub fn new(number: u64) -> Self {
        Self {
            number,
            hash: None,
            parent_hash: B256::ZERO,
            timestamp: 0,
            transactions: Vec::new(),
        }
    }

    pub fn hash(mut self, hash: B256) -> Self {
        self.hash = Some(hash);
        self
    }

    pub fn parent_hash(mut self, parent_hash: B256) -> Self {
        self.parent_hash = parent_hash;
        self
    }

    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn transactions(mut self, transactions: Vec<B256>) -> Self {
        self.transactions = transactions;
        self
    }

    pub fn add_transaction(mut self, tx_hash: B256) -> Self {
        self.transactions.push(tx_hash);
        self
    }

    pub fn build(self) -> Block {
        Block {
            number: self.number,
            hash: self.hash.unwrap_or_else(B256::random),
            parent_hash: self.parent_hash,
            timestamp: self.timestamp,
            transactions: self.transactions,
        }
    }
}

pub struct GenesisAccount {
    pub address: Address,
    pub balance: U256,
}

pub struct Genesis {
    pub accounts: Vec<GenesisAccount>,
    pub timestamp: u64,
}

impl Default for Genesis {
    fn default() -> Self {
        let accounts = vec![
            GenesisAccount { address: Address::repeat_byte(0x1), balance: U256::from(100000000000000000000u128) }, // 100 ETH
            GenesisAccount { address: Address::repeat_byte(0x2), balance: U256::from(100000000000000000000u128) }, // 100 ETH
            GenesisAccount { address: Address::repeat_byte(0x3), balance: U256::from(100000000000000000000u128) }, // 100 ETH
            GenesisAccount { address: Address::repeat_byte(0x4), balance: U256::from(100000000000000000000u128) }, // 100 ETH
        ];

        Self {
            accounts,
            timestamp: 1640995200, // Jan 1st 2022
        }
    }
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
            block_gas_limit: EvmU256::max_value(),
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
            contracts: BTreeMap::new(),
        };

        // Create genesis block
        let genesis_block = Block::builder(0)
            .timestamp(genesis.timestamp)
            .build();
        storage.add_block(genesis_block);

        // Pre-fund accounts
        for account in genesis.accounts {
            storage.set_balance(account.address, account.balance);
        }

        storage
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
