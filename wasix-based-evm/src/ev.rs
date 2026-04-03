use alloy_primitives::{Address, B256, U256 as AlloyU256};
pub use evm_interpreter::uint::{H160, H256, U256 as EvmU256};
pub use evm;
// pub use evm_precompile;

pub fn address_to_h160(address: Address) -> H160 {
    H160(address.0 .0)
}

#[allow(dead_code)]
pub fn h160_to_address(h160: H160) -> Address {
    Address::from(h160.0)
}

#[allow(dead_code)]
pub fn b256_to_h256(b256: B256) -> H256 {
    H256(b256.0)
}

#[allow(dead_code)]
pub fn h256_to_b256(h256: H256) -> B256 {
    B256::from(h256.0)
}

#[allow(dead_code)]
pub fn alloy_u256_to_evm_u256(u256: AlloyU256) -> EvmU256 {
    EvmU256(u256.into_limbs())
}

#[allow(dead_code)]
pub fn evm_u256_to_alloy_u256(u256: EvmU256) -> AlloyU256 {
    AlloyU256::from_limbs(u256.0)
}
