use alloy_consensus::{Block, ReceiptWithBloom as Receipt, TxEnvelope, Header, Transaction as _, transaction::SignerRecoverable as _};
use alloy_primitives::{logs_bloom, Log, B256, Address, LogData};
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

#[derive(Clone)]
pub struct Executor {
    pub config: Config,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            config: Config::shanghai(),
        }
    }

    pub fn execute(&self, storage: &mut InMemoryStorage, tx: TxEnvelope, block: Block<TxEnvelope>) -> Result<TransactValue, String> {
        self.run_execution(storage, vec![tx], block, true).map(|mut v| v.remove(0))
    }

    pub fn call(&self, storage: &InMemoryStorage, tx: TxEnvelope, block: Block<TxEnvelope>) -> Result<TransactValue, String> {
        let mut storage_copy = storage.clone();
        self.run_execution(&mut storage_copy, vec![tx], block, false).map(|mut v| v.remove(0))
    }

    pub fn execute_with_changeset(&self, storage: &mut InMemoryStorage, transactions: Vec<TxEnvelope>, block: Block<TxEnvelope>) -> Result<(Vec<TransactValue>, Vec<Receipt>, OverlayedChangeSet), String> {
        info!("[Executor] Executing {} transactions for block {}", transactions.len(), block.header.number);
        let precompiles = StandardPrecompileSet;
        let etable = evm::interpreter::etable::Chained(ExecutionEtable::new(), GasometerEtable::new());
        let resolver = EtableResolver::new(&precompiles, &etable);
        let invoker = Invoker::new(&resolver);

        // Update backend environment for the current block
        storage.backend.environment.block_number = EvmU256::from(block.header.number);
        storage.backend.environment.block_timestamp = EvmU256::from(block.header.timestamp);

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
            let sender = tx.recover_signer().map_err(|e| format!("Failed to recover signer: {:?}", e))?;
            debug!("[Executor] DEBUG: Executing tx: from={:?}, to={:?}, value={:?}, gas_limit={:?}", sender, tx.to(), tx.value(), tx.gas_limit());
            let gas_price = TransactGasPrice::Legacy(EvmU256::from(tx.gas_price().unwrap_or_default()));
            
            let call_create = match tx.to() {
                Some(to) => TransactArgsCallCreate::Call {
                    address: H160::from_slice(to.as_slice()),
                    data: tx.input().to_vec(),
                },
                None => TransactArgsCallCreate::Create {
                    init_code: tx.input().to_vec(),
                    salt: None,
                },
            };

            let args = TransactArgs {
                caller: H160::from_slice(sender.as_slice()),
                value: EvmU256::from_big_endian(&tx.value().to_be_bytes::<32>()),
                gas_limit: EvmU256::from(tx.gas_limit()),
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
                    info!("[Executor] Transaction executed successfully: hash={:?}, used_gas={:?}", tx.hash(), value.used_gas);
                    cumulative_gas_used += value.used_gas.as_u64();
                    
                    let (_, changeset) = overlayed.deconstruct();
                    total_changeset.merge(changeset.clone());

                    let logs: Vec<Log> = changeset.logs.into_iter().map(|l| Log {
                        address: Address::from(l.address.0),
                        data: LogData::new_unchecked(
                            l.topics.into_iter().map(|t| B256::from(t.0)).collect(),
                            l.data.into(),
                        ),
                    }).collect();
                    let bloom = logs_bloom(logs.iter());
                    
                    let status = match value.call_create {
                        evm::standard::TransactValueCallCreate::Call { .. } => {
                            alloy_consensus::Eip658Value::success()
                        }
                        evm::standard::TransactValueCallCreate::Create { .. } => {
                            alloy_consensus::Eip658Value::success()
                        }
                    };
                    let receipt = Receipt {
                        receipt: alloy_consensus::Receipt {
                            status,
                            cumulative_gas_used,
                            logs,
                        },
                        logs_bloom: bloom,
                    };
                    receipts.push(receipt);
                    results.push(value);
                }
                Err(e) => {
                    info!("[Executor] Transaction execution failed: hash={:?}, error={:?}", tx.hash(), e);
                    return Err(format!("Transaction execution failed: {:?}", e));
                }
            }
        }

        Ok((results, receipts, total_changeset))
    }

    pub fn execute_block(&self, storage: &mut InMemoryStorage, transactions: Vec<TxEnvelope>, block: Block<TxEnvelope>) -> Result<Vec<TransactValue>, String> {
        let (results, receipts, total_changeset) = self.execute_with_changeset(storage, transactions.clone(), block.clone())?;
        
        // Finalize block with correct roots
        let cumulative_gas_used = receipts.last().map(|r| r.receipt.cumulative_gas_used).unwrap_or(0);
        
        let bloom = logs_bloom(receipts.iter().flat_map(|r| r.receipt.logs.iter()));
        
        let withdrawals = block.body.withdrawals.clone().unwrap_or_default();

        // Apply changeset to storage to calculate state root
        let mut storage_for_root = storage.clone();
        storage_for_root.backend.apply_overlayed(&total_changeset);
        let state_root = storage_for_root.calculate_state_root();

        let txs_root = alloy_trie::root::ordered_trie_root(&transactions);
        let withdrawals_root = alloy_trie::root::ordered_trie_root(&withdrawals);
        let receipts_root = alloy_trie::root::ordered_trie_root(&receipts);

        debug!("[Executor] Block finalization: state_root={:?}, transactions_root={:?}, receipts_root={:?}, withdrawals_root={:?}", state_root, txs_root, receipts_root, withdrawals_root);

        // Verify roots against block
        if block.header.state_root != B256::ZERO && state_root != block.header.state_root {
            let err = format!("State root mismatch: expected {:?}, got {:?}", block.header.state_root, state_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }
        if block.header.transactions_root != B256::ZERO && txs_root != block.header.transactions_root {
            let err = format!("Transactions root mismatch: expected {:?}, got {:?}", block.header.transactions_root, txs_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }
        if block.header.receipts_root != B256::ZERO && receipts_root != block.header.receipts_root {
            let err = format!("Receipts root mismatch: expected {:?}, got {:?}", block.header.receipts_root, receipts_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }
        if block.header.withdrawals_root.unwrap_or(B256::ZERO) != B256::ZERO && withdrawals_root != block.header.withdrawals_root.unwrap_or(B256::ZERO) {
            let err = format!("Withdrawals root mismatch: expected {:?}, got {:?}", block.header.withdrawals_root, withdrawals_root);
            info!("[Executor] ERROR: {}", err);
            return Err(err);
        }

        let finalized_block = Block {
            header: Header {
                number: block.header.number,
                parent_hash: block.header.parent_hash,
                ommers_hash: block.header.ommers_hash,
                beneficiary: block.header.beneficiary,
                state_root,
                transactions_root: txs_root,
                receipts_root,
                logs_bloom: bloom,
                difficulty: block.header.difficulty,
                gas_limit: block.header.gas_limit,
                gas_used: cumulative_gas_used,
                timestamp: block.header.timestamp,
                extra_data: block.header.extra_data.clone(),
                mix_hash: block.header.mix_hash,
                nonce: block.header.nonce,
                base_fee_per_gas: block.header.base_fee_per_gas,
                withdrawals_root: Some(withdrawals_root),
                blob_gas_used: block.header.blob_gas_used,
                excess_blob_gas: block.header.excess_blob_gas,
                parent_beacon_block_root: block.header.parent_beacon_block_root,
                ..Default::default()
            },
            body: alloy_consensus::BlockBody {
                transactions: transactions.clone(),
                ommers: Vec::new(),
                withdrawals: Some(withdrawals),
            },
        };

        // Apply total changeset to actual storage
        storage.backend.apply_overlayed(&total_changeset);

        // Add transactions, receipts and block to storage
        for (tx, receipt) in transactions.into_iter().zip(receipts.into_iter()) {
            let hash = *tx.hash();
            storage.add_transaction(tx);
            storage.add_receipt(hash, receipt);
        }
        storage.add_block(finalized_block);
        info!("[Executor] Block finalized and saved to storage: number={:?}", block.header.number);

        Ok(results)
    }

    fn run_execution(&self, storage: &mut InMemoryStorage, transactions: Vec<TxEnvelope>, block: Block<TxEnvelope>, apply_changes: bool) -> Result<Vec<TransactValue>, String> {
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
                let sender = tx.recover_signer().map_err(|e| format!("Failed to recover signer: {:?}", e))?;
                let gas_price = TransactGasPrice::Legacy(EvmU256::from(tx.gas_price().unwrap_or_default()));
                let call_create = match tx.to() {
                    Some(to) => TransactArgsCallCreate::Call {
                        address: H160::from_slice(to.as_slice()),
                        data: tx.input().to_vec(),
                    },
                    None => TransactArgsCallCreate::Create {
                        init_code: tx.input().to_vec(),
                        salt: None,
                    },
                };
                let args = TransactArgs {
                    caller: H160::from_slice(sender.as_slice()),
                    value: EvmU256::from_big_endian(&tx.value().to_be_bytes::<32>()),
                    gas_limit: EvmU256::from(tx.gas_limit()),
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


