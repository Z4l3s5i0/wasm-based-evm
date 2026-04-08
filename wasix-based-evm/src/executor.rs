use alloy_primitives::B256;
use crate::ev::{H160, EvmU256, evm};
use evm::backend::OverlayedChangeSet;
use crate::{info, debug};
use evm::{
    transact,
    backend::OverlayedBackend,
    standard::{Config, Invoker, ExecutionEtable, GasometerEtable, TransactArgs, TransactArgsCallCreate, TransactGasPrice, EtableResolver, TransactValue},
};
use evm_precompile::StandardPrecompileSet;
use crate::storage::storage::InMemoryStorage;
use crate::storage::types::{logs_bloom, Block, Log, Receipt, Transaction, Withdrawal};

pub trait OverlayedChangeSetExt {
    fn merge(&mut self, other: OverlayedChangeSet);
}

impl OverlayedChangeSetExt for OverlayedChangeSet {
    fn merge(&mut self, other: OverlayedChangeSet) {
        self.logs.extend(other.logs);
        self.balances.extend(other.balances);
        self.codes.extend(other.codes);
        self.nonces.extend(other.nonces);
        self.storage_resets.extend(other.storage_resets);
        self.storages.extend(other.storages);
        self.transient_storage.extend(other.transient_storage);
        self.accessed.extend(other.accessed);
        self.touched.extend(other.touched);
        self.deletes.extend(other.deletes);
    }
}

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
        self.run_execution(storage, vec![tx], block, true).map(|mut v| v.remove(0))
    }

    pub fn call(&self, storage: &InMemoryStorage, tx: Transaction, block: Block) -> Result<TransactValue, String> {
        let mut storage_copy = storage.clone();
        self.run_execution(&mut storage_copy, vec![tx], block, false).map(|mut v| v.remove(0))
    }

    pub fn execute_with_changeset(&self, storage: &mut InMemoryStorage, transactions: Vec<Transaction>, block: Block) -> Result<(Vec<TransactValue>, Vec<Receipt>, OverlayedChangeSet), String> {
        let precompiles = StandardPrecompileSet;
        let etable = evm::interpreter::etable::Chained(ExecutionEtable::new(), GasometerEtable::new());
        let resolver = EtableResolver::new(&precompiles, &etable);
        let invoker = Invoker::new(&resolver);

        // Update backend environment for the current block
        storage.backend.environment.block_number = EvmU256::from(block.body.execution_payload.block_number);
        storage.backend.environment.block_timestamp = EvmU256::from(block.body.execution_payload.timestamp);

        let mut results = Vec::new();
        let mut receipts = Vec::new();
        let mut cumulative_gas_used = 0u64;

        let mut total_changeset = OverlayedChangeSet {
            logs: Vec::new(),
            balances: std::collections::BTreeMap::new(),
            codes: std::collections::BTreeMap::new(),
            nonces: std::collections::BTreeMap::new(),
            storage_resets: std::collections::BTreeSet::new(),
            storages: std::collections::BTreeMap::new(),
            transient_storage: std::collections::BTreeMap::new(),
            accessed: std::collections::BTreeSet::new(),
            touched: std::collections::BTreeSet::new(),
            deletes: std::collections::BTreeSet::new(),
        };

        for tx in &transactions {
            debug!("[Executor] DEBUG: Executing tx: from={:?}, to={:?}, value={:?}, gas_limit={:?}", tx.from, tx.to, tx.value, tx.gas_limit);
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

            // Use an overlay that includes changes from previous transactions in the same block
            let mut current_backend = storage.backend.clone();
            current_backend.apply_overlayed(&total_changeset);
            let mut overlayed = OverlayedBackend::new(&current_backend, &self.config.runtime);

            let result = transact(
                args,
                None,
                &mut overlayed,
                &invoker,
            );

            match result {
                Ok(value) => {
                    info!("[Executor] Transaction executed successfully: hash={:?}, used_gas={:?}", tx.hash, value.used_gas);
                    cumulative_gas_used += value.used_gas.as_u64();
                    
                    let (_, changeset) = overlayed.deconstruct();
                    total_changeset.merge(changeset.clone());

                    let logs: Vec<Log> = changeset.logs.into_iter().map(Log::from).collect();
                    let bloom = logs_bloom(&logs);
                    let success = match value.call_create {
                        crate::ev::evm::standard::TransactValueCallCreate::Call { .. } => true,
                        crate::ev::evm::standard::TransactValueCallCreate::Create { .. } => true,
                    };
                    let receipt = Receipt {
                        success,
                        cumulative_gas_used,
                        logs_bloom: bloom,
                        logs,
                    };
                    receipts.push(receipt);
                    results.push(value);
                }
                Err(e) => {
                    info!("[Executor] Transaction execution failed: hash={:?}, error={:?}", tx.hash, e);
                    return Err(format!("Transaction execution failed: {:?}", e));
                }
            }
        }

        Ok((results, receipts, total_changeset))
    }

    pub fn execute_block(&self, storage: &mut InMemoryStorage, transactions: Vec<Transaction>, block: Block) -> Result<Vec<TransactValue>, String> {
        let (results, receipts, total_changeset) = self.execute_with_changeset(storage, transactions.clone(), block.clone())?;
        
        // Finalize block with correct roots
        let cumulative_gas_used = receipts.last().map(|r| r.cumulative_gas_used).unwrap_or(0);
        
        let mut block_builder = Block::builder(block.slot)
            .proposer_index(block.proposer_index)
            .parent_root(block.parent_root)
            .parent_hash(block.body.execution_payload.parent_hash)
            .fee_recipient(block.body.execution_payload.fee_recipient)
            .prev_randao(block.body.execution_payload.prev_randao)
            .block_number(block.body.execution_payload.block_number)
            .gas_limit(block.body.execution_payload.gas_limit)
            .gas_used(cumulative_gas_used)
            .timestamp(block.body.execution_payload.timestamp)
            .extra_data(block.body.execution_payload.extra_data.clone())
            .base_fee_per_gas(block.body.execution_payload.base_fee_per_gas);

        for tx in &transactions {
            block_builder = block_builder.add_transaction(tx.clone());
        }

        for receipt in &receipts {
            block_builder = block_builder.add_receipt(receipt.clone());
        }

        let bloom = logs_bloom(&receipts.iter().flat_map(|r| r.logs.clone()).collect::<Vec<_>>());
        block_builder = block_builder.logs_bloom(bloom.as_slice().to_vec());

        for withdrawal in &block.body.execution_payload.withdrawals {
            block_builder = block_builder.withdrawals(vec![withdrawal.clone()]);
        }

        // Apply changeset to storage to calculate state root
        let mut storage_for_root = storage.clone();
        storage_for_root.backend.apply_overlayed(&total_changeset);
        let state_root = storage_for_root.calculate_state_root();

        let txs_root = Transaction::calculate_root(&transactions);
        let withdrawals_root = Withdrawal::calculate_root(&block.body.execution_payload.withdrawals);
        let receipts_root = Receipt::calculate_root(&receipts);

        debug!("[Executor] Block finalization: state_root={:?}, transactions_root={:?}, receipts_root={:?}, withdrawals_root={:?}", state_root, txs_root, receipts_root, withdrawals_root);

        // Verify roots against block
        if block.body.execution_payload.state_root != B256::ZERO && state_root != block.body.execution_payload.state_root {
            let err = format!("State root mismatch: expected {:?}, got {:?}", block.body.execution_payload.state_root, state_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }
        if block.body.execution_payload.transactions_root != B256::ZERO && txs_root != block.body.execution_payload.transactions_root {
            let err = format!("Transactions root mismatch: expected {:?}, got {:?}", block.body.execution_payload.transactions_root, txs_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }
        if block.body.execution_payload.receipts_root != B256::ZERO && receipts_root != block.body.execution_payload.receipts_root {
            let err = format!("Receipts root mismatch: expected {:?}, got {:?}", block.body.execution_payload.receipts_root, receipts_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }
        if block.body.execution_payload.withdrawals_root != B256::ZERO && withdrawals_root != block.body.execution_payload.withdrawals_root {
            let err = format!("Withdrawals root mismatch: expected {:?}, got {:?}", block.body.execution_payload.withdrawals_root, withdrawals_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }

        let finalized_block = block_builder
            .state_root(state_root)
            .transactions_root(txs_root)
            .withdrawals_root(withdrawals_root)
            .receipts_root(receipts_root)
            .build();

        // Apply total changeset to actual storage
        storage.backend.apply_overlayed(&total_changeset);

        // Add transactions, receipts and block to storage
        for (tx, receipt) in transactions.into_iter().zip(receipts.into_iter()) {
            let tx_hash = tx.hash;
            storage.add_transaction(tx);
            storage.add_receipt(tx_hash, receipt);
        }
        storage.add_block(finalized_block);
        info!("[Executor] Block finalized and saved to storage: slot={:?}", block.slot);

        Ok(results)
    }

    fn run_execution(&self, storage: &mut InMemoryStorage, transactions: Vec<Transaction>, block: Block, apply_changes: bool) -> Result<Vec<TransactValue>, String> {
        if apply_changes {
            self.execute_block(storage, transactions, block)
        } else {
            // Just for call/dry-run, we don't need the complex block logic
            let precompiles = StandardPrecompileSet;
            let etable = evm::interpreter::etable::Chained(ExecutionEtable::new(), GasometerEtable::new());
            let resolver = EtableResolver::new(&precompiles, &etable);
            let invoker = Invoker::new(&resolver);

            let mut results = Vec::new();
            for tx in &transactions {
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
                let mut overlayed = OverlayedBackend::new(&storage.backend, &self.config.runtime);
                let result = transact(args, None, &mut overlayed, &invoker);
                match result {
                    Ok(value) => results.push(value),
                    Err(e) => return Err(format!("Transaction execution failed: {:?}", e)),
                }
            }
            Ok(results)
        }
    }
}


