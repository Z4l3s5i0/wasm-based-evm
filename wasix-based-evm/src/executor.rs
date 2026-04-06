use crate::ev::{H160, EvmU256, evm};
use crate::storage::{InMemoryStorage, Transaction, Block, Receipt, Log};
use evm::{
    transact,
    backend::OverlayedBackend,
    standard::{Config, Invoker, ExecutionEtable, GasometerEtable, TransactArgs, TransactArgsCallCreate, TransactGasPrice, EtableResolver, TransactValue},
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

    pub fn execute(&self, storage: &mut InMemoryStorage, tx: Transaction, block: Block) -> Result<TransactValue, String> {
        self.run_execution(storage, tx, block, true)
    }

    pub fn call(&self, storage: &InMemoryStorage, tx: Transaction, block: Block) -> Result<TransactValue, String> {
        let mut storage_copy = storage.clone();
        self.run_execution(&mut storage_copy, tx, block, false)
    }

    fn run_execution(&self, storage: &mut InMemoryStorage, tx: Transaction, block: Block, apply_changes: bool) -> Result<TransactValue, String> {
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
        storage.backend.environment.block_number = EvmU256::from(block.body.execution_payload.block_number);
        storage.backend.environment.block_timestamp = EvmU256::from(block.body.execution_payload.timestamp);

        let mut overlayed = OverlayedBackend::new(&storage.backend, &self.config.runtime);

        let result = transact(
            args,
            None,
            &mut overlayed,
            &invoker,
        );

        match result {
            Ok(value) => {
                if apply_changes {
                    let (_, changeset) = overlayed.deconstruct();
                    storage.backend.apply_overlayed(&changeset);

                    let logs: Vec<Log> = changeset.logs.into_iter().map(Log::from).collect();
                    let bloom = crate::storage::logs_bloom(&logs);
                    let success = match value.call_create {
                        crate::ev::evm::standard::TransactValueCallCreate::Call { .. } => true,
                        crate::ev::evm::standard::TransactValueCallCreate::Create { .. } => true,
                    };
                    let receipt = Receipt {
                        success,
                        cumulative_gas_used: value.used_gas.as_u64(), // Simple approach: one tx per block for now
                        logs_bloom: bloom,
                        logs,
                    };

                    // Finalize block with correct roots
                    let mut block_builder = Block::builder(block.slot)
                        .proposer_index(block.proposer_index)
                        .parent_root(block.parent_root)
                        .parent_hash(block.body.execution_payload.parent_hash)
                        .fee_recipient(block.body.execution_payload.fee_recipient)
                        .prev_randao(block.body.execution_payload.prev_randao)
                        .block_number(block.body.execution_payload.block_number)
                        .gas_limit(block.body.execution_payload.gas_limit)
                        .gas_used(value.used_gas.as_u64())
                        .timestamp(block.body.execution_payload.timestamp)
                        .extra_data(block.body.execution_payload.extra_data.clone())
                        .base_fee_per_gas(block.body.execution_payload.base_fee_per_gas)
                        .logs_bloom(bloom.as_slice().to_vec())
                        .add_transaction(tx.clone());

                    for withdrawal in &block.body.execution_payload.withdrawals {
                        block_builder = block_builder.withdrawals(vec![withdrawal.clone()]);
                    }

                    let state_root = storage.calculate_state_root();
                    let tx_list = vec![tx.clone()];
                    let txs_root = InMemoryStorage::calculate_transactions_root(&tx_list);
                    let withdrawals_root = InMemoryStorage::calculate_withdrawals_root(&block.body.execution_payload.withdrawals);
                    let receipts_root = InMemoryStorage::calculate_receipts_root(&[receipt.clone()]);

                    let finalized_block = block_builder
                        .state_root(state_root)
                        .transactions_root(txs_root)
                        .withdrawals_root(withdrawals_root)
                        .receipts_root(receipts_root)
                        .build();

                    // Add transaction, receipt and block to storage
                    storage.add_transaction(tx.clone());
                    storage.add_receipt(tx.hash, receipt);
                    storage.add_block(finalized_block);
                }
                Ok(value)
            }
            Err(e) => Err(format!("Transaction execution failed: {:?}", e)),
        }
    }
}


