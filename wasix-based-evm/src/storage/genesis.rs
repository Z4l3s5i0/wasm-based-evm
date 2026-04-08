use std::collections::BTreeMap;
use alloy_primitives::{Address, B256, U256};
use alloy_genesis::Genesis as AlloyGenesis;

pub struct GenesisAccount {
    pub address: Address,
    pub balance: U256,
    pub code: Option<Vec<u8>>,
    pub nonce: Option<u64>,
    pub storage: Option<BTreeMap<B256, B256>>,
}

pub struct Genesis {
    pub accounts: Vec<GenesisAccount>,
    pub timestamp: u64,
    pub chain_id: u64,
    pub gas_limit: u64,
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
        let accounts = vec![
            GenesisAccount {
                address: Address::repeat_byte(0x1),
                balance: U256::from(100000000000000000000u128),
                code: None,
                nonce: None,
                storage: None,
            }, // 100 ETH
            GenesisAccount {
                address: Address::repeat_byte(0x2),
                balance: U256::from(100000000000000000000u128),
                code: None,
                nonce: None,
                storage: None,
            }, // 100 ETH
            GenesisAccount {
                address: Address::repeat_byte(0x3),
                balance: U256::from(100000000000000000000u128),
                code: None,
                nonce: None,
                storage: None,
            }, // 100 ETH
            GenesisAccount {
                address: Address::repeat_byte(0x4),
                balance: U256::from(100000000000000000000u128),
                code: None,
                nonce: None,
                storage: None,
            }, // 100 ETH
        ];

        Self {
            accounts,
            timestamp: 1640995200, // Jan 1st 2022
            chain_id: 1,
            gas_limit: 30000000,
        }
    }
}