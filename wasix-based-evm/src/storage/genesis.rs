use std::collections::BTreeMap;
use alloy_primitives::{Address, B256, U256};
use alloy_genesis::Genesis as AlloyGenesis;

#[derive(Default, Clone)]
pub struct GenesisAccount {
    pub address: Address,
    pub balance: U256,
    pub code: Option<Vec<u8>>,
    pub nonce: Option<u64>,
    pub storage: Option<BTreeMap<B256, B256>>,
}

impl GenesisAccount {
    pub fn builder() -> GenesisAccountBuilder {
        GenesisAccountBuilder::default()
    }
}

#[derive(Default)]
pub struct GenesisAccountBuilder {
    address: Address,
    balance: U256,
    code: Option<Vec<u8>>,
    nonce: Option<u64>,
    storage: Option<BTreeMap<B256, B256>>,
}

impl GenesisAccountBuilder {
    pub fn address(mut self, address: Address) -> Self {
        self.address = address;
        self
    }
    pub fn balance(mut self, balance: U256) -> Self {
        self.balance = balance;
        self
    }
    pub fn code(mut self, code: Vec<u8>) -> Self {
        self.code = Some(code);
        self
    }
    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = Some(nonce);
        self
    }
    pub fn storage(mut self, storage: BTreeMap<B256, B256>) -> Self {
        self.storage = Some(storage);
        self
    }
    pub fn build(self) -> GenesisAccount {
        GenesisAccount {
            address: self.address,
            balance: self.balance,
            code: self.code,
            nonce: self.nonce,
            storage: self.storage,
        }
    }
}

#[derive(Clone)]
pub struct Genesis {
    pub accounts: Vec<GenesisAccount>,
    pub timestamp: u64,
    pub chain_id: u64,
    pub gas_limit: u64,
}

impl Genesis {
    pub fn builder() -> GenesisBuilder {
        GenesisBuilder::default()
    }
}

#[derive(Default)]
pub struct GenesisBuilder {
    accounts: Vec<GenesisAccount>,
    timestamp: u64,
    chain_id: u64,
    gas_limit: u64,
}

impl GenesisBuilder {
    pub fn add_account(mut self, account: GenesisAccount) -> Self {
        self.accounts.push(account);
        self
    }
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
    pub fn chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = chain_id;
        self
    }
    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }
    pub fn build(self) -> Genesis {
        Genesis {
            accounts: self.accounts,
            timestamp: self.timestamp,
            chain_id: self.chain_id,
            gas_limit: self.gas_limit,
        }
    }
}

impl From<AlloyGenesis> for Genesis {
    fn from(alloy_genesis: AlloyGenesis) -> Self {
        let accounts = alloy_genesis.alloc.into_iter().map(|(address, account)| {
            GenesisAccount {
                address,
                balance: account.balance,
                code: account.code.map(|c| c.to_vec()),
                nonce: account.nonce,
                storage: account.storage.map(|s| {
                    s.into_iter().map(|(k, v)| (B256::from(k), B256::from(v))).collect()
                }),
            }
        }).collect();

        Self {
            accounts,
            timestamp: alloy_genesis.timestamp,
            chain_id: alloy_genesis.config.chain_id,
            gas_limit: alloy_genesis.gas_limit,
        }
    }
}

impl Default for Genesis {
    fn default() -> Self {

        Self {
            accounts: vec![],
            timestamp: 1640995200, // Jan 1st 2022
            chain_id: 1,
            gas_limit: 30000000,
        }
    }
}