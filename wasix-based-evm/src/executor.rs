use crate::ev::{H160, EvmU256, evm};
use crate::storage::{InMemoryStorage, Transaction, Block};
use evm::{
    transact,
    backend::OverlayedBackend,
    standard::{Config, Invoker, ExecutionEtable, GasometerEtable, TransactArgs, TransactArgsCallCreate, TransactGasPrice, EtableResolver},
};
use evm_precompile::StandardPrecompileSet;

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

