use crate::ev::{H160, H256, EvmU256, evm, address_to_h160, alloy_u256_to_evm_u256, h160_to_address};
use crate::{info, debug};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, InMemoryAccount};
use evm::interpreter::runtime::Log as EvmLog;
use alloy_primitives::{Address, FixedBytes, B256, U256, keccak256, Bloom, BloomInput};
use alloy_genesis::Genesis as AlloyGenesis;
use alloy_rlp::{RlpEncodable, RlpDecodable, Encodable};
use alloy_trie::{TrieAccount, root::ordered_trie_root};
use alloy_trie::root::{state_root_unhashed, storage_root_unsorted};
use std::collections::BTreeMap;
use rand::RngCore;

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Receipt {
    pub success: bool,
    pub cumulative_gas_used: u64,
    pub logs_bloom: Bloom,
    pub logs: Vec<Log>,
}

impl Receipt {
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Log {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Vec<u8>,
}

impl From<EvmLog> for Log {
    fn from(evm_log: EvmLog) -> Self {
        Self {
            address: h160_to_address(evm_log.address),
            topics: evm_log.topics.into_iter().map(|t| B256::from(t.0)).collect(),
            data: evm_log.data,
        }
    }
}

pub fn logs_bloom(logs: &[Log]) -> Bloom {
    let mut bloom = Bloom::ZERO;
    for log in logs {
        bloom.accrue(BloomInput::Raw(log.address.as_slice()));
        for topic in &log.topics {
            bloom.accrue(BloomInput::Raw(topic.as_slice()));
        }
    }
    bloom
}

fn random_b256() -> B256 {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    FixedBytes::<32>(buf)
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
#[rlp(trailing)]
pub struct Transaction {
    pub hash: B256,
    pub nonce: u64,
    pub from: Address,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub to: Option<Address>,
}

impl Transaction {
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }
}

impl Transaction {
    pub fn builder(from: Address) -> TransactionBuilder {
        TransactionBuilder::new(from)
    }
}

pub struct TransactionBuilder {
    hash: Option<B256>,
    nonce: u64,
    from: Address,
    to: Option<Address>,
    value: U256,
    data: Vec<u8>,
    gas_limit: u64,
    gas_price: U256,
}

impl TransactionBuilder {
    pub fn new(from: Address) -> Self {
        Self {
            hash: None,
            nonce: 0,
            from,
            to: None,
            value: U256::ZERO,
            data: Vec::new(),
            gas_limit: 21000,
            gas_price: U256::ZERO,
        }
    }

    pub fn hash(mut self, hash: B256) -> Self {
        self.hash = Some(hash);
        self
    }

    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = nonce;
        self
    }

    pub fn to(mut self, to: Option<Address>) -> Self {
        self.to = to;
        self
    }

    pub fn value(mut self, value: U256) -> Self {
        self.value = value;
        self
    }

    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    pub fn gas_price(mut self, gas_price: U256) -> Self {
        self.gas_price = gas_price;
        self
    }

    pub fn build(self) -> Transaction {
        Transaction {
            hash: self.hash.unwrap_or_else(random_b256),
            nonce: self.nonce,
            from: self.from,
            to: self.to,
            value: self.value,
            data: self.data,
            gas_limit: self.gas_limit,
            gas_price: self.gas_price,
        }
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Withdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64,
}

impl Withdrawal {
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Eth1Data {
    pub deposit_root: B256,
    pub deposit_count: u64,
    pub block_hash: B256,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct ProposerSlashing {
    // Placeholder fields for ProposerSlashing
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct AttesterSlashing {
    // Placeholder fields for AttesterSlashing
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Attestation {
    // TODO: Implement Attestation
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Deposit {
    // Placeholder fields for Deposit
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct VoluntaryExit {
    // Placeholder fields for VoluntaryExit
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct SyncAggregate {
    pub sync_committee_bits: Vec<u8>,
    pub sync_committee_signature: Vec<u8>,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct ExecutionPayload {
    pub parent_hash: B256,
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    pub logs_bloom: Vec<u8>,
    pub prev_randao: B256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub base_fee_per_gas: u128,
    pub block_hash: B256,
    pub transactions_root: B256,
    pub withdrawals_root: B256,

    pub transactions: Vec<Transaction>,
    pub withdrawals: Vec<Withdrawal>,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct ExecutionPayloadHeader {
    pub parent_hash: B256,
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    pub logs_bloom: Vec<u8>,
    pub prev_randao: B256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub base_fee_per_gas: u128,
    pub block_hash: B256,
    pub transactions_root: B256,
    pub withdrawals_root: B256,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct BlockBody {
    pub randao_reveal: B256,
    pub eth1_data: Eth1Data,
    pub graffiti: Vec<u8>,

    pub proposer_slashings: Vec<ProposerSlashing>,
    pub attester_slashings: Vec<AttesterSlashing>,
    pub attestations: Vec<Attestation>,

    pub deposits: Vec<Deposit>,
    pub voluntary_exits: Vec<VoluntaryExit>,
    pub sync_aggregate: SyncAggregate,

    pub execution_payload: ExecutionPayload,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Block {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BlockBody,
}

impl Block {
    pub fn builder(slot: u64) -> BlockBuilder {
        BlockBuilder::new(slot)
    }
}

pub struct BlockBuilder {
    slot: u64,
    proposer_index: u64,
    parent_root: B256,
    state_root: B256,
    body: Option<BlockBody>,

    // ExecutionPayload fields for convenience
    parent_hash: B256,
    fee_recipient: Address,
    receipts_root: B256,
    logs_bloom: Vec<u8>,
    prev_randao: B256,
    block_number: u64,
    gas_limit: u64,
    gas_used: u64,
    timestamp: u64,
    extra_data: Vec<u8>,
    base_fee_per_gas: u128,
    block_hash: B256,
    transactions_root: B256,
    withdrawals_root: B256,
    receipts: Vec<Receipt>,
    transactions: Vec<Transaction>,
    withdrawals: Vec<Withdrawal>,
}

impl BlockBuilder {
    pub fn new(slot: u64) -> Self {
        Self {
            slot,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root: B256::ZERO,
            body: None,

            parent_hash: B256::ZERO,
            fee_recipient: Address::ZERO,
            receipts_root: B256::ZERO,
            logs_bloom: Vec::new(),
            prev_randao: B256::ZERO,
            block_number: slot, // Default block number to slot
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 0,
            extra_data: Vec::new(),
            base_fee_per_gas: 0,
            block_hash: B256::ZERO,
            transactions_root: B256::ZERO,
            withdrawals_root: B256::ZERO,
            receipts: Vec::new(),
            transactions: Vec::new(),
            withdrawals: Vec::new(),
        }
    }

    pub fn transactions_root(mut self, transactions_root: B256) -> Self {
        self.transactions_root = transactions_root;
        self
    }

    pub fn withdrawals_root(mut self, withdrawals_root: B256) -> Self {
        self.withdrawals_root = withdrawals_root;
        self
    }

    pub fn proposer_index(mut self, proposer_index: u64) -> Self {
        self.proposer_index = proposer_index;
        self
    }

    pub fn parent_root(mut self, parent_root: B256) -> Self {
        self.parent_root = parent_root;
        self
    }

    pub fn state_root(mut self, state_root: B256) -> Self {
        self.state_root = state_root;
        self
    }

    pub fn body(mut self, body: BlockBody) -> Self {
        self.body = Some(body);
        self
    }

    pub fn parent_hash(mut self, parent_hash: B256) -> Self {
        self.parent_hash = parent_hash;
        self
    }

    pub fn fee_recipient(mut self, fee_recipient: Address) -> Self {
        self.fee_recipient = fee_recipient;
        self
    }

    pub fn receipts_root(mut self, receipts_root: B256) -> Self {
        self.receipts_root = receipts_root;
        self
    }

    pub fn logs_bloom(mut self, logs_bloom: Vec<u8>) -> Self {
        self.logs_bloom = logs_bloom;
        self
    }

    pub fn prev_randao(mut self, prev_randao: B256) -> Self {
        self.prev_randao = prev_randao;
        self
    }

    pub fn block_number(mut self, block_number: u64) -> Self {
        self.block_number = block_number;
        self
    }

    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = gas_limit;
        self
    }

    pub fn gas_used(mut self, gas_used: u64) -> Self {
        self.gas_used = gas_used;
        self
    }

    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn extra_data(mut self, extra_data: Vec<u8>) -> Self {
        self.extra_data = extra_data;
        self
    }

    pub fn base_fee_per_gas(mut self, base_fee_per_gas: u128) -> Self {
        self.base_fee_per_gas = base_fee_per_gas;
        self
    }

    pub fn block_hash(mut self, block_hash: B256) -> Self {
        self.block_hash = block_hash;
        self
    }

    pub fn transactions(mut self, transactions: Vec<Transaction>) -> Self {
        self.transactions = transactions;
        self
    }

    pub fn add_transaction(mut self, transaction: Transaction) -> Self {
        self.transactions.push(transaction);
        self
    }

    pub fn receipts(mut self, receipts: Vec<Receipt>) -> Self {
        self.receipts = receipts;
        self
    }

    pub fn add_receipt(mut self, receipt: Receipt) -> Self {
        self.receipts.push(receipt);
        self
    }

    pub fn withdrawals(mut self, withdrawals: Vec<Withdrawal>) -> Self {
        self.withdrawals = withdrawals;
        self
    }

    pub fn build(self) -> Block {
        let transactions_root = if self.transactions_root == B256::ZERO && !self.transactions.is_empty() {
            InMemoryStorage::calculate_transactions_root(&self.transactions)
        } else {
            self.transactions_root
        };

        let withdrawals_root = if self.withdrawals_root == B256::ZERO && !self.withdrawals.is_empty() {
            InMemoryStorage::calculate_withdrawals_root(&self.withdrawals)
        } else {
            self.withdrawals_root
        };

        let receipts_root = if self.receipts_root == B256::ZERO && !self.receipts.is_empty() {
            InMemoryStorage::calculate_receipts_root(&self.receipts)
        } else {
            self.receipts_root
        };

        let body = self.body.unwrap_or_else(|| BlockBody {
            randao_reveal: B256::ZERO,
            eth1_data: Eth1Data {
                deposit_root: B256::ZERO,
                deposit_count: 0,
                block_hash: B256::ZERO,
            },
            graffiti: vec![0u8; 32],
            proposer_slashings: Vec::new(),
            attester_slashings: Vec::new(),
            attestations: Vec::new(),
            deposits: Vec::new(),
            voluntary_exits: Vec::new(),
            sync_aggregate: SyncAggregate {
                sync_committee_bits: Vec::new(),
                sync_committee_signature: Vec::new(),
            },
            execution_payload: ExecutionPayload {
                parent_hash: self.parent_hash,
                fee_recipient: self.fee_recipient,
                state_root: self.state_root,
                receipts_root,
                logs_bloom: self.logs_bloom,
                prev_randao: self.prev_randao,
                block_number: self.block_number,
                gas_limit: self.gas_limit,
                gas_used: self.gas_used,
                timestamp: self.timestamp,
                extra_data: self.extra_data,
                base_fee_per_gas: self.base_fee_per_gas,
                block_hash: if self.block_hash == B256::ZERO { random_b256() } else { self.block_hash },
                transactions_root,
                withdrawals_root,
                transactions: self.transactions,
                withdrawals: self.withdrawals,
            },
        });

        Block {
            slot: self.slot,
            proposer_index: self.proposer_index,
            parent_root: self.parent_root,
            state_root: self.state_root,
            body,
        }
    }
}

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

#[derive(Clone)]
pub struct InMemoryStorage {
    pub backend: InMemoryBackend,
    pub blocks: BTreeMap<u64, Block>,
    pub transactions: BTreeMap<B256, Transaction>,
    pub receipts: BTreeMap<B256, Receipt>,
    #[allow(dead_code)]
    pub contracts: BTreeMap<Address, Vec<u8>>,
    pub mempool: crate::mempool::Mempool,
    pub head_block_hash: B256,
    pub safe_block_hash: B256,
    pub finalized_block_hash: B256,
}

impl InMemoryStorage {
    pub fn new(chain_id: EvmU256) -> Self {
        Self::new_with_genesis(chain_id, Genesis::default())
    }

    pub fn new_with_genesis(chain_id: EvmU256, genesis: Genesis) -> Self {
        let env = InMemoryEnvironment {
            block_hashes: BTreeMap::new(),
            block_number: EvmU256::zero(),
            block_coinbase: H160::zero(),
            block_timestamp: EvmU256::from(genesis.timestamp),
            block_difficulty: EvmU256::zero(),
            block_randomness: None,
            block_gas_limit: EvmU256::from(genesis.gas_limit),
            block_base_fee_per_gas: EvmU256::zero(),
            blob_base_fee_per_gas: EvmU256::zero(),
            blob_versioned_hashes: Vec::new(),
            chain_id,
        };

        let mut storage = Self {
            backend: InMemoryBackend {
                environment: env,
                state: BTreeMap::new(),
            },
            blocks: BTreeMap::new(),
            transactions: BTreeMap::new(),
            receipts: BTreeMap::new(),
            contracts: BTreeMap::new(),
            mempool: crate::mempool::Mempool::new(U256::ZERO),
            head_block_hash: B256::ZERO,
            safe_block_hash: B256::ZERO,
            finalized_block_hash: B256::ZERO,
        };

        // Create genesis block
        let genesis_block_builder = Block::builder(0)
            .timestamp(genesis.timestamp)
            .gas_limit(genesis.gas_limit);

        // Pre-fund and initialize accounts
        for account in genesis.accounts {
            let mut storage_map = BTreeMap::new();
            if let Some(s) = account.storage {
                for (k, v) in s {
                    storage_map.insert(H256(k.0), H256(v.0));
                }
            }
            let im_account = InMemoryAccount {
                balance: alloy_u256_to_evm_u256(account.balance),
                nonce: EvmU256::from(account.nonce.unwrap_or(0)),
                code: account.code.unwrap_or_default(),
                storage: storage_map,
                transient_storage: Default::default(),
            };
            storage.backend.state.insert(address_to_h160(account.address), im_account);
        }

        let state_root = storage.calculate_state_root();
        let genesis_block = genesis_block_builder.state_root(state_root).build();
        let genesis_hash = genesis_block.body.execution_payload.block_hash;
        storage.add_block(genesis_block);
        storage.head_block_hash = genesis_hash;
        storage.safe_block_hash = genesis_hash;
        storage.finalized_block_hash = genesis_hash;

        storage
    }

    pub fn add_block(&mut self, block: Block) {
        let block_number = block.body.execution_payload.block_number;
        let block_hash = block.body.execution_payload.block_hash;
        debug!("[Storage] Adding block #{} with hash {:?}", block_number, block_hash);
        self.blocks.insert(block_number, block);
        self.head_block_hash = block_hash;
    }

    pub fn add_transaction(&mut self, tx: Transaction) {
        debug!("[Storage] Adding transaction {:?}", tx.hash);
        self.transactions.insert(tx.hash, tx);
    }

    pub fn add_receipt(&mut self, tx_hash: B256, receipt: Receipt) {
        debug!("[Storage] Adding receipt for transaction {:?}", tx_hash);
        self.receipts.insert(tx_hash, receipt);
    }

    pub fn get_receipt_by_tx_hash(&self, tx_hash: B256) -> Option<&Receipt> {
        self.receipts.get(&tx_hash)
    }

    pub fn get_block_by_number(&self, number: u64) -> Option<&Block> {
        self.blocks.get(&number)
    }

    pub fn get_block_by_hash(&self, hash: B256) -> Option<&Block> {
        self.blocks.values().find(|b| b.body.execution_payload.block_hash == hash)
    }

    pub fn get_transaction_by_hash(&self, hash: B256) -> Option<&Transaction> {
        self.transactions.get(&hash)
    }

    pub fn get_block_by_transaction_hash(&self, tx_hash: B256) -> Option<&Block> {
        self.blocks.values().find(|b| b.body.execution_payload.transactions.iter().any(|tx| tx.hash == tx_hash))
    }

    pub fn get_block_receipts(&self, block_hash: B256) -> Vec<Receipt> {
        let block = match self.get_block_by_hash(block_hash) {
            Some(b) => b,
            None => return Vec::new(),
        };
        block.body.execution_payload.transactions.iter()
            .filter_map(|tx| self.receipts.get(&tx.hash).cloned())
            .collect()
    }

    pub fn get_latest_block(&self) -> Option<&Block> {
        self.blocks.values().last()
    }

    pub fn get_latest_block_number(&self) -> u64 {
        self.blocks.keys().last().cloned().unwrap_or(0)
    }

    /// Get a mutable reference to the backend.
    pub fn backend_mut(&mut self) -> &mut InMemoryBackend {
        &mut self.backend
    }

    /// Set the backend.
    pub fn set_backend(&mut self, backend: InMemoryBackend) {
        self.backend = backend;
    }

    pub fn get_balance(&self, address: Address) -> U256 {
        let h160 = H160::from_slice(address.as_slice());
        if let Some(a) = self.backend.state.get(&h160) {
            let mut bytes = [0u8; 32];
            a.balance.to_big_endian(&mut bytes);
            U256::from_be_bytes(bytes)
        } else {
            U256::ZERO
        }
    }

    pub fn get_accounts(&self) -> Vec<Address> {
        self.backend.state.keys().map(|h| Address::from(h.0)).collect()
    }

    pub fn get_code(&self, address: Address) -> Vec<u8> {
        self.backend.state.get(&H160::from_slice(address.as_slice()))
            .map(|a| a.code.clone())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn set_contract_code(&mut self, address: Address, code: Vec<u8>) {
        debug!("[Storage] Setting contract code for address {:?}", address);
        self.contracts.insert(address, code.clone());
        self.backend.state.entry(H160::from_slice(address.as_slice())).or_insert(InMemoryAccount {
            balance: EvmU256::zero(),
            code: code.clone(),
            nonce: EvmU256::zero(),
            storage: BTreeMap::<H256, H256>::new(),
            transient_storage: BTreeMap::<H256, H256>::new(),
        }).code = code.clone();
    }

    pub fn set_balance(&mut self, address: Address, balance: U256) {
        debug!("[Storage] Setting balance for address {:?} to {}", address, balance);
        let bytes = balance.to_be_bytes::<32>();
        let evm_balance = EvmU256::from_big_endian(&bytes);
        let h160 = H160::from_slice(address.as_slice());
        self.backend.state.entry(h160).or_insert(InMemoryAccount {
            balance: evm_balance,
            code: Vec::new(),
            nonce: EvmU256::zero(),
            storage: BTreeMap::<H256, H256>::new(),
            transient_storage: BTreeMap::<H256, H256>::new(),
        }).balance = evm_balance;
    }

    pub fn calculate_state_root(&self) -> B256 {
        state_root_unhashed(self.backend.state.iter().map(|(addr, acc)| {
            let storage_root = storage_root_unsorted(acc.storage.iter().map(|(k, v)| (B256::from(k.0), U256::from_be_bytes(v.0))));

            let trie_acc = TrieAccount {
                nonce: acc.nonce.as_u64(),
                balance: {
                    let mut b = [0u8; 32];
                    acc.balance.to_big_endian(&mut b);
                    U256::from_be_bytes(b)
                },
                storage_root,
                code_hash: keccak256(&acc.code),
            };
            (Address::from(addr.0), trie_acc)
        }))
    }

    pub fn calculate_transactions_root(transactions: &[Transaction]) -> B256 {
        ordered_trie_root(transactions)
    }

    pub fn calculate_withdrawals_root(withdrawals: &[Withdrawal]) -> B256 {
        ordered_trie_root(withdrawals)
    }

    pub fn calculate_receipts_root(receipts: &[Receipt]) -> B256 {
        ordered_trie_root(receipts)
    }
}
