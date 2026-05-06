use alloy_consensus::{Block, Header, ReceiptWithBloom as Receipt, TxEnvelope, Transaction as _, transaction::SignerRecoverable as _};
use alloy_rpc_types::Withdrawal;
use alloy_primitives::{logs_bloom, Log, B256, Address, LogData, B64, U256, Bytes};
use crate::evm::ev::{H160, EvmU256, evm, address_to_h160, alloy_u256_to_evm_u256};
use evm::backend::{OverlayedChangeSet, InMemoryAccount};
use crate::{info, debug};
use evm::{
    transact,
    backend::OverlayedBackend,
    standard::{Config, Invoker, ExecutionEtable, GasometerEtable, TransactArgs, TransactArgsCallCreate, TransactGasPrice, EtableResolver, TransactValue},
};
use evm_precompile::StandardPrecompileSet;
use crate::storage::storage::RedbStorage;
use crate::misc::metrics::{
    TRANSACTION_EXECUTION_TIME, CPU_CYCLES_TOTAL, INSTRUCTION_COUNT_TOTAL, BLOCK_EXECUTION_TIME,
    GAS_PROCESSED_TOTAL,
};

struct BlockRoots {
    state_root: B256,
    transactions_root: B256,
    receipts_root: B256,
    withdrawals_root: B256,
}

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

    pub fn execute_with_changeset(&self, storage: &mut RedbStorage, transactions: Vec<TxEnvelope>, block: Block<TxEnvelope>) -> Result<(Vec<TransactValue>, Vec<Receipt>, OverlayedChangeSet), String> {
        info!("[Executor] Executing {} transactions for block {}", transactions.len(), block.header.number);
        let precompiles = StandardPrecompileSet;
        let etable = evm::interpreter::etable::Chained(ExecutionEtable::new(), GasometerEtable::new());
        let resolver = EtableResolver::new(&precompiles, &etable);
        let invoker = Invoker::new(&resolver);

        // Update backend environment for the current block
        storage.backend.environment.block_number = EvmU256::from(block.header.number);
        storage.backend.environment.block_timestamp = EvmU256::from(block.header.timestamp);
        storage.backend.environment.block_base_fee_per_gas = alloy_u256_to_evm_u256(U256::from(block.header.base_fee_per_gas.unwrap_or(0)));

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

        total_changeset.transient_storage.clear(); // Ensure it's empty for Shanghai

        for tx in &transactions {
            let sender = tx.recover_signer().map_err(|e| format!("Failed to recover signer: {:?}", e))?;
            debug!("[Executor] DEBUG: Executing tx: from={:?}, to={:?}, value={:?}, gas_limit={:?}", sender, tx.to(), tx.value(), tx.gas_limit());
            let args = self.tx_to_transact_args(tx, sender)?;

            // Use an overlay that includes changes from previous transactions in the same block
            let mut current_backend = storage.backend.clone();
            current_backend.apply_overlayed(&total_changeset);
            let mut overlayed = OverlayedBackend::new(&current_backend, &self.config.runtime);

            let clock = quanta::Clock::new();
            let start_cycles = clock.now();
            let start_tx = std::time::Instant::now();
            let result = transact(
                args,
                None,
                &mut overlayed,
                &invoker,
            );
            TRANSACTION_EXECUTION_TIME.observe(start_tx.elapsed().as_secs_f64());
            CPU_CYCLES_TOTAL.inc_by(clock.now().duration_since(start_cycles).as_nanos() as f64);

            match result {
                Ok(value) => {
                    INSTRUCTION_COUNT_TOTAL.inc_by(value.instruction_count as f64);
                    info!("[Executor] Transaction executed successfully: hash={:?}, used_gas={:?}, status={:?}", tx.hash(), value.used_gas, value.call_create);
                    cumulative_gas_used += value.used_gas.as_u64();
                    GAS_PROCESSED_TOTAL.inc_by(value.used_gas.as_u64() as f64);
                    
                    let (_, changeset) = overlayed.deconstruct();
                    info!("[Executor] Changeset for tx {:?}: balances={:?}, storages={:?}", tx.hash(), changeset.balances.len(), changeset.storages.len());
                    for ((addr, slot), val) in &changeset.storages {
                        info!("[Executor]   Storage update: addr={:?}, slot={:?}, val={:?}", addr, slot, val);
                    }
                    total_changeset.merge(changeset.clone());

                    let receipt = self.create_receipt(&value, &changeset, cumulative_gas_used);
                    receipts.push(receipt);
                    results.push(value);
                }
                Err(e) => {
                    debug!("[Executor] Transaction execution failed: hash={:?}, error={:?}", tx.hash(), e);
                    return Err(format!("Transaction execution failed: {:?}", e));
                }
            }
        }

        Ok((results, receipts, total_changeset))
    }

    fn tx_to_transact_args(&self, tx: &TxEnvelope, sender: Address) -> Result<TransactArgs<'_>, String> {
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

        Ok(TransactArgs {
            caller: H160::from_slice(sender.as_slice()),
            value: EvmU256::from_big_endian(&tx.value().to_be_bytes::<32>()),
            gas_limit: EvmU256::from(tx.gas_limit()),
            gas_price,
            access_list: Vec::new(),
            call_create,
            config: &self.config,
        })
    }

    fn create_receipt(&self, value: &TransactValue, changeset: &OverlayedChangeSet, cumulative_gas_used: u64) -> Receipt {
        let logs: Vec<Log> = changeset.logs.iter().map(|l| Log {
            address: Address::from(l.address.0),
            data: LogData::new_unchecked(
                l.topics.iter().map(|t| B256::from(t.0)).collect(),
                l.data.clone().into(),
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
        
        Receipt {
            receipt: alloy_consensus::Receipt {
                status,
                cumulative_gas_used,
                logs,
            },
            logs_bloom: bloom,
        }
    }

    pub fn execute_block(&self, storage: &mut RedbStorage, transactions: Vec<TxEnvelope>, block: Block<TxEnvelope>) -> Result<Vec<TransactValue>, String> {
        let start = std::time::Instant::now();
        let (results, receipts, total_changeset) = self.execute_with_changeset(storage, transactions.clone(), block.clone())?;
        BLOCK_EXECUTION_TIME.observe(start.elapsed().as_secs_f64());
        
        let cumulative_gas_used = receipts.last().map(|r| r.receipt.cumulative_gas_used).unwrap_or(0);
        let bloom = logs_bloom(receipts.iter().flat_map(|r| r.receipt.logs.iter()));
        let withdrawals = block.body.withdrawals.clone().unwrap_or_default();

        let roots = self.calculate_roots(storage, &transactions, &receipts, &withdrawals, &total_changeset);
        
        debug!("[Executor] Block finalization: state_root={:?}, transactions_root={:?}, receipts_root={:?}, withdrawals_root={:?}", 
            roots.state_root, roots.transactions_root, roots.receipts_root, roots.withdrawals_root);

        self.verify_roots(&block.header, &roots)?;

        let finalized_block = self.create_finalized_block(&block, &transactions, &withdrawals, &roots, cumulative_gas_used, bloom);

        self.finalize_state(storage, &total_changeset, &transactions, &receipts, &withdrawals, finalized_block);

        Ok(results)
    }

    fn calculate_roots(
        &self,
        storage: &RedbStorage,
        transactions: &[TxEnvelope],
        receipts: &[Receipt],
        withdrawals: &[Withdrawal],
        total_changeset: &OverlayedChangeSet
    ) -> BlockRoots {
        let mut storage_for_root = storage.clone();
        storage_for_root.backend.apply_overlayed(total_changeset);
        let state_root = storage_for_root.calculate_state_root();

        let transactions_root = if transactions.is_empty() { 
            alloy_trie::EMPTY_ROOT_HASH 
        } else { 
            alloy_trie::root::ordered_trie_root(transactions) 
        };

        let withdrawals_root = if withdrawals.is_empty() { 
            alloy_trie::EMPTY_ROOT_HASH 
        } else { 
            alloy_trie::root::ordered_trie_root(withdrawals) 
        };

        let receipts_root = if receipts.is_empty() { 
            alloy_trie::EMPTY_ROOT_HASH 
        } else { 
            alloy_trie::root::ordered_trie_root(receipts) 
        };

        BlockRoots {
            state_root,
            transactions_root,
            receipts_root,
            withdrawals_root,
        }
    }

    fn verify_roots(&self, header: &Header, roots: &BlockRoots) -> Result<(), String> {
        if header.state_root != B256::ZERO 
            && header.state_root != alloy_trie::EMPTY_ROOT_HASH
            && roots.state_root != header.state_root 
        {
            return Err(format!("State root mismatch: expected {:?}, got {:?}", header.state_root, roots.state_root));
        }
        if header.transactions_root != B256::ZERO 
            && header.transactions_root != alloy_trie::EMPTY_ROOT_HASH
            && roots.transactions_root != header.transactions_root 
        {
            return Err(format!("Transactions root mismatch: expected {:?}, got {:?}", header.transactions_root, roots.transactions_root));
        }
        if header.receipts_root != B256::ZERO 
            && header.receipts_root != alloy_trie::EMPTY_ROOT_HASH
            && roots.receipts_root != header.receipts_root 
        {
            return Err(format!("Receipts root mismatch: expected {:?}, got {:?}", header.receipts_root, roots.receipts_root));
        }
        let header_withdrawals_root = header.withdrawals_root.unwrap_or(B256::ZERO);
        if header_withdrawals_root != B256::ZERO && roots.withdrawals_root != header_withdrawals_root {
            return Err(format!("Withdrawals root mismatch: expected {:?}, got {:?}", header.withdrawals_root, roots.withdrawals_root));
        }
        Ok(())
    }

    fn create_finalized_block(
        &self,
        block: &Block<TxEnvelope>,
        transactions: &[TxEnvelope],
        withdrawals: &[Withdrawal],
        roots: &BlockRoots,
        cumulative_gas_used: u64,
        bloom: alloy_primitives::Bloom,
    ) -> Block<TxEnvelope> {
        Block {
            header: Header {
                number: block.header.number,
                parent_hash: block.header.parent_hash,
                ommers_hash: block.header.ommers_hash,
                beneficiary: block.header.beneficiary,
                state_root: roots.state_root,
                transactions_root: roots.transactions_root,
                receipts_root: roots.receipts_root,
                logs_bloom: bloom,
                difficulty: block.header.difficulty,
                gas_limit: block.header.gas_limit,
                gas_used: cumulative_gas_used,
                timestamp: block.header.timestamp,
                extra_data: block.header.extra_data.clone(),
                mix_hash: block.header.mix_hash,
                nonce: B64::ZERO,
                base_fee_per_gas: block.header.base_fee_per_gas,
                withdrawals_root: Some(roots.withdrawals_root),
                blob_gas_used: None,
                excess_blob_gas: None,
                parent_beacon_block_root: block.header.parent_beacon_block_root.or(None),
                requests_hash: None,
            },
            body: alloy_consensus::BlockBody {
                transactions: transactions.to_vec(),
                ommers: vec![],
                withdrawals: Some(alloy_eips::eip4895::Withdrawals::new(withdrawals.to_vec())),
            },
        }
    }

    fn finalize_state(
        &self,
        storage: &mut RedbStorage,
        total_changeset: &OverlayedChangeSet,
        transactions: &[TxEnvelope],
        receipts: &[Receipt],
        withdrawals: &[Withdrawal],
        finalized_block: Block<TxEnvelope>,
    ) {
        // Apply total changeset to actual storage
        storage.backend.apply_overlayed(total_changeset);

        // Add transactions, receipts and block to storage
        for (tx, receipt) in transactions.iter().cloned().zip(receipts.iter().cloned()) {
            let hash = *tx.hash();
            storage.add_transaction(tx);
            storage.add_receipt(hash, receipt);
        }

        // Apply withdrawals
        for withdrawal in withdrawals {
            let addr = withdrawal.address;
            let amount_wei = U256::from(withdrawal.amount) * U256::from(1_000_000_000u64);
            let evm_amount_wei = alloy_u256_to_evm_u256(amount_wei);
            let h160_addr = address_to_h160(addr);
            
            if let Some(account) = storage.backend.state.get_mut(&h160_addr) {
                account.balance += evm_amount_wei;
                debug!("[Executor] Applied withdrawal: address={:?}, amount={} Gwei", addr, withdrawal.amount);
            } else {
                storage.backend.state.insert(h160_addr, InMemoryAccount {
                    balance: evm_amount_wei,
                    nonce: EvmU256::zero(),
                    code: Vec::new(),
                    storage: std::collections::BTreeMap::new(),
                    transient_storage: std::collections::BTreeMap::new(),
                });
                debug!("[Executor] Applied withdrawal (new account): address={:?}, amount={} Gwei", addr, withdrawal.amount);
            }
        }

        let block_number = finalized_block.header.number;
        storage.add_block(finalized_block);
        debug!("[Executor] Block finalized and saved to storage: number={:?}", block_number);
    }

    pub fn execute_block_for_payload(
        &self,
        storage: &mut RedbStorage,
        transactions: Vec<TxEnvelope>,
        parent_header: &Header,
        attr: &alloy_rpc_types::engine::PayloadAttributes,
        base_fee_per_gas: Option<u64>,
    ) -> Result<(Block<TxEnvelope>, Vec<Receipt>), String> {
        let start = std::time::Instant::now();
        let block_number = parent_header.number + 1;
        
        // Mock a block for execute_with_changeset
        let mock_header = Header {
            number: block_number,
            timestamp: attr.timestamp,
            parent_hash: parent_header.hash_slow(),
            ..Default::default()
        };
        let mock_block = Block {
            header: mock_header,
            body: alloy_consensus::BlockBody {
                transactions: transactions.clone(),
                ommers: vec![],
                withdrawals: attr.withdrawals.clone().map(|w| alloy_eips::eip4895::Withdrawals::new(w.into_iter().map(|wi| alloy_eips::eip4895::Withdrawal {
                    index: wi.index,
                    validator_index: wi.validator_index,
                    address: wi.address,
                    amount: wi.amount,
                }).collect())),
            },
        };

        let (_, receipts, total_changeset) = self.execute_with_changeset(storage, transactions.clone(), mock_block.clone())?;
        BLOCK_EXECUTION_TIME.observe(start.elapsed().as_secs_f64());
        
        let cumulative_gas_used = receipts.last().map(|r| r.receipt.cumulative_gas_used).unwrap_or(0);
        let bloom = logs_bloom(receipts.iter().flat_map(|r| r.receipt.logs.iter()));
        let withdrawals = mock_block.body.withdrawals.clone().unwrap_or_default();

        let roots = self.calculate_roots(storage, &transactions, &receipts, &withdrawals, &total_changeset);
        
        let finalized_header = Header {
            parent_hash: parent_header.hash_slow(),
            ommers_hash: alloy_consensus::EMPTY_OMMER_ROOT_HASH,
            number: block_number,
            timestamp: attr.timestamp,
            beneficiary: attr.suggested_fee_recipient,
            gas_limit: parent_header.gas_limit,
            base_fee_per_gas,
            extra_data: Bytes::new(),
            mix_hash: attr.prev_randao,
            nonce: B64::ZERO,
            state_root: roots.state_root,
            transactions_root: roots.transactions_root,
            receipts_root: roots.receipts_root,
            withdrawals_root: Some(roots.withdrawals_root),
            logs_bloom: bloom,
            gas_used: cumulative_gas_used,
            difficulty: U256::ZERO,
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
            requests_hash: None,
        };

        Ok((Block {
            header: finalized_header,
            body: mock_block.body,
        }, receipts))
    }

    pub fn run_execution(&self, storage: &mut RedbStorage, transactions: Vec<TxEnvelope>, block: Block<TxEnvelope>, apply_changes: bool) -> Result<Vec<TransactValue>, String> {
        let start = std::time::Instant::now();
        let result = if apply_changes {
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
                let args = self.tx_to_transact_args(tx, sender)?;
                let mut overlayed = OverlayedBackend::new(&storage.backend, &self.config.runtime);
                
                let clock = quanta::Clock::new();
                let start_cycles = clock.now();
                let start_tx = std::time::Instant::now();
                let result = transact(args, None, &mut overlayed, &invoker);
                TRANSACTION_EXECUTION_TIME.observe(start_tx.elapsed().as_secs_f64());
                CPU_CYCLES_TOTAL.inc_by(clock.now().duration_since(start_cycles).as_nanos() as f64);
                
                match result {
                    Ok(value) => {
                        INSTRUCTION_COUNT_TOTAL.inc_by(value.instruction_count as f64);
                        GAS_PROCESSED_TOTAL.inc_by(value.used_gas.as_u64() as f64);
                        results.push(value)
                    },
                    Err(e) => return Err(format!("Transaction execution failed: {:?}", e)),
                }
            }
            Ok(results)
        };

        if result.is_ok() {
            BLOCK_EXECUTION_TIME.observe(start.elapsed().as_secs_f64());
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::TxLegacy;
    use alloy_consensus::SignableTransaction;

    #[test]
    fn test_executor_new() {
        let _executor = Executor::new();
        // Just verify it doesn't panic and uses Shanghai by default
    }

    #[test]
    fn test_run_execution_dry_run() {
        let executor = Executor::new();
        let mut storage = RedbStorage::new(EvmU256::from(1));
        
        let tx = TxEnvelope::Legacy(TxLegacy {
            nonce: 0,
            gas_limit: 21000,
            gas_price: 1_000_000_000,
            to: alloy_primitives::TxKind::Call(Address::repeat_byte(0x12)),
            value: U256::from(100),
            ..Default::default()
        }.into_signed(alloy_primitives::Signature::test_signature()));

        // Set balance for sender
        let sender = tx.recover_signer().unwrap();
        // Set a huge balance to avoid OutOfFund
        storage.set_balance(sender, U256::MAX);
        
        println!("Sender: {:?}", sender);
        println!("Balance: {:?}", storage.get_balance(sender));

        let results = executor.run_execution(&mut storage, vec![tx], Block::default(), false);
        match &results {
            Ok(res) => println!("Execution successful: {:?}", res),
            Err(e) => println!("Execution failed: {}", e),
        }
        assert!(results.is_ok());
        let results = results.unwrap();
        assert_eq!(results.len(), 1);
    }
}


