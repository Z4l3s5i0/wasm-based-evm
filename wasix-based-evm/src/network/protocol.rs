use tentacle::{
    traits::ServiceProtocol,
    context::{ProtocolContext, ProtocolContextMutRef},
    bytes::{Bytes, BytesMut},
};
use alloy_rlp::{RlpEncodable, RlpDecodable, Encodable, Decodable, BufMut};
use alloy_primitives::{B256, U256};
use crate::storage::types::{Block, Transaction};
use tentacle::ProtocolId;

pub const ETH_PROTOCOL_ID: ProtocolId = ProtocolId::new(1);

/// Ethereum `eth` protocol message IDs
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageId {
    Status = 0x00,
    NewPooledTransactionHashes = 0x08,
    Transactions = 0x02,
    GetBlockHeaders = 0x03,
    BlockHeaders = 0x04,
    GetBlockBodies = 0x05,
    BlockBodies = 0x06,
    NewBlock = 0x07,
    GetPooledTransactions = 0x09,
    PooledTransactions = 0x0a,
}

/// `Status` message (eth/66+)
#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Status {
    pub protocol_version: u32,
    pub network_id: u64,
    pub total_difficulty: U256,
    pub best_hash: B256,
    pub genesis_hash: B256,
}

/// `Transactions` message
#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct Transactions(pub Vec<Transaction>);

/// `NewPooledTransactionHashes` message
#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct NewPooledTransactionHashes(pub Vec<B256>);

/// `NewBlock` message
#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct NewBlock {
    pub block: Block,
    pub total_difficulty: U256,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct GetBlockHeaders {
    pub request_id: u64,
    pub block: BlockHashOrNumber,
    pub amount: u64,
    pub skip: u64,
    pub reverse: bool,
}

#[derive(Debug, Clone)]
pub enum BlockHashOrNumber {
    Hash(B256),
    Number(u64),
}

impl Encodable for BlockHashOrNumber {
    fn encode(&self, out: &mut dyn BufMut) {
        match self {
            BlockHashOrNumber::Hash(hash) => hash.encode(out),
            BlockHashOrNumber::Number(num) => num.encode(out),
        }
    }

    fn length(&self) -> usize {
        match self {
            BlockHashOrNumber::Hash(hash) => hash.length(),
            BlockHashOrNumber::Number(num) => num.length(),
        }
    }
}

impl Decodable for BlockHashOrNumber {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        if buf.is_empty() {
            return Err(alloy_rlp::Error::InputTooShort);
        }

        // Try to decode as B256 first (32 bytes), then as u64
        // In Ethereum RLP, a 32-byte hash and a number have different prefixes
        let first_byte = buf[0];
        if first_byte == 0xa0 { // RLP prefix for 32-byte string
            Ok(BlockHashOrNumber::Hash(B256::decode(buf)?))
        } else {
            Ok(BlockHashOrNumber::Number(u64::decode(buf)?))
        }
    }
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct BlockHeaders {
    pub request_id: u64,
    pub headers: Vec<Block>,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct GetBlockBodies {
    pub request_id: u64,
    pub hashes: Vec<B256>,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct BlockBodies {
    pub request_id: u64,
    pub bodies: Vec<Block>,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct GetPooledTransactions {
    pub request_id: u64,
    pub hashes: Vec<B256>,
}

#[derive(Debug, Clone, RlpEncodable, RlpDecodable)]
pub struct PooledTransactions {
    pub request_id: u64,
    pub transactions: Vec<Transaction>,
}

