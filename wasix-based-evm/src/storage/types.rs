use alloy_primitives::{Address, Bloom, BloomInput, FixedBytes, B256, U256};
use alloy_rlp::{Encodable, RlpDecodable, RlpEncodable};
use discv5::enr::k256::elliptic_curve::rand_core::RngCore;
use crate::ev::h160_to_address;
use crate::storage::storage::InMemoryStorage;
use evm::interpreter::runtime::Log as EvmLog;


fn random_b256() -> B256 {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    FixedBytes::<32>(buf)
}
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