use crate::ev::{H160, H256, EvmU256, evm};
use crate::storage::{InMemoryStorage, Transaction, Block};
use evm::uint::U256Ext;
use evm::{
    transact,
    backend::OverlayedBackend,
    standard::{Config, Invoker, ExecutionEtable, GasometerEtable, TransactArgs, TransactArgsCallCreate, TransactGasPrice, EtableResolver},
};
use evm_precompile::StandardPrecompileSet;
use alloy_primitives::{B256, U256};

pub struct Executor {
    pub config: Config,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            config: Config::shanghai(),
        }
    }

    pub fn execute(&self, storage: &mut InMemoryStorage, tx: Transaction, block: Block) -> Result<(), String> {
        let precompiles = StandardPrecompileSet;
        let etable = evm::interpreter::etable::Chained(ExecutionEtable::new(), GasometerEtable::new());
        let resolver = EtableResolver::new(&precompiles, &etable);
        let invoker = Invoker::new(&resolver);

        let gas_price = TransactGasPrice::Legacy(EvmU256::from_big_endian(&tx.gas_price.to_be_bytes::<32>()));
        
        let call_create = match tx.to {
            Some(to) => TransactArgsCallCreate::Call {
                address: H160::from_slice(to.as_slice()),
                data: tx.data.clone(),
            },
            None => TransactArgsCallCreate::Create {
                init_code: tx.data.clone(),
                salt: None,
            },
        };

        let args = TransactArgs {
            caller: H160::from_slice(tx.from.as_slice()),
            value: EvmU256::from_big_endian(&tx.value.to_be_bytes::<32>()),
            gas_limit: EvmU256::from(tx.gas_limit),
            gas_price,
            access_list: Vec::new(),
            call_create,
            config: &self.config,
        };

        // Update backend environment for the current block
        storage.backend.environment.block_number = EvmU256::from(block.number);
        storage.backend.environment.block_timestamp = EvmU256::from(block.timestamp);

        let mut overlayed = OverlayedBackend::new(&storage.backend, &self.config.runtime);

        let result = transact(
            args,
            None,
            &mut overlayed,
            &invoker,
        );

        match result {
            Ok(_) => {
                let (_, changeset) = overlayed.deconstruct();
                storage.backend.apply_overlayed(&changeset);
                // Add transaction and block to storage
                storage.add_transaction(tx);
                storage.add_block(block);
                Ok(())
            }
            Err(e) => Err(format!("Transaction execution failed: {:?}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::InMemoryStorage;
    use crate::ev::{H160, EvmU256};
    use alloy_primitives::{address, uint};

    #[test]
    fn test_executor_and_storage() {
        let chain_id = EvmU256::from(1);
        let mut storage = InMemoryStorage::new(chain_id);
        let executor = Executor::new();

        let from_addr = address!("0000000000000000000000000000000000000001");
        let to_addr = address!("0000000000000000000000000000000000000002");
        
        let initial_balance = uint!(1000000000000000000_U256); // 1 ETH
        storage.set_balance(from_addr, initial_balance);

        let transfer_value = uint!(100000000000000000_U256); // 0.1 ETH
        let tx = Transaction {
            hash: B256::from_slice(&[1u8; 32]),
            nonce: 0,
            from: from_addr,
            to: Some(to_addr),
            value: transfer_value,
            data: Vec::new(),
            gas_limit: 100000,
            gas_price: uint!(1000000000_U256), // 1 Gwei
        };

        let block = Block {
            number: 1,
            hash: B256::from_slice(&[2u8; 32]),
            parent_hash: B256::ZERO,
            timestamp: 123456789,
            transactions: vec![tx.hash],
        };

        let result = executor.execute(&mut storage, tx.clone(), block.clone());
        assert!(result.is_ok(), "Execution failed: {:?}", result.err());

        // Check balances
        let from_evm_addr = H160::from_slice(from_addr.as_slice());
        let to_evm_addr = H160::from_slice(to_addr.as_slice());
        let from_acc = storage.backend.state.get(&from_evm_addr).unwrap();
        let to_acc = storage.backend.state.get(&to_evm_addr).unwrap();

        let initial_evm_balance = EvmU256::from_big_endian(&initial_balance.to_be_bytes::<32>());
        let transfer_evm_value = EvmU256::from_big_endian(&transfer_value.to_be_bytes::<32>());

        assert!(from_acc.balance < initial_evm_balance);
        assert_eq!(to_acc.balance, transfer_evm_value);

        // Check storage for tx and block
        assert!(storage.transactions.contains_key(&tx.hash));
        assert!(storage.blocks.contains_key(&block.number));
    }
}
