use alloy_primitives::{Address, U256};

pub struct AccountMapper;

impl AccountMapper {
    pub fn to_hex(value: U256) -> String {
        format!("0x{:x}", value)
    }

    pub fn addresses_to_rpc(addresses: Vec<Address>) -> Vec<String> {
        addresses.into_iter().map(|a| format!("{:#x}", a)).collect()
    }
}