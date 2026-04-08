use tentacle::{
    builder::MetaBuilder,
    context::{ProtocolContext, ProtocolContextMutRef},
    service::{ProtocolHandle, ProtocolMeta},
    traits::ServiceProtocol,
    ProtocolId,
    bytes::{Bytes, BytesMut},
};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::network::NetworkMessage;
use tokio::sync::mpsc;
use crate::{info, debug};
use alloy_rlp::{RlpEncodable, RlpDecodable, Encodable, Decodable, BufMut};
use alloy_primitives::{B256, U256};
use crate::network::sync::SyncEvent;
use crate::storage::storage::InMemoryStorage;
use crate::storage::types::{Block, Transaction};

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


struct EthProtocolHandler {
    storage: Arc<Mutex<InMemoryStorage>>,
    sync_send: mpsc::Sender<SyncEvent>,
    network_send: mpsc::Sender<NetworkMessage>,
}

#[async_trait::async_trait]
impl ServiceProtocol for EthProtocolHandler {
    async fn init(&mut self, context: &mut ProtocolContext) {
        debug!("[EthProtocol] initiated on protocol: {}", context.proto_id);
    }

    async fn connected(&mut self, context: ProtocolContextMutRef<'_>, version: &str) {
        info!("[EthProtocol] connected on session: {}, version: {}", context.session.id, version);
        
        // Send Status message immediately
        let storage = self.storage.lock().await;
        let latest_block = storage.get_latest_block();
        let status = Status {
            protocol_version: 66, // Example version
            network_id: 1,      // Mainnet for example
            total_difficulty: U256::ZERO, // Simplified
            best_hash: latest_block.map(|b| b.body.execution_payload.block_hash).unwrap_or_default(),
            genesis_hash: B256::ZERO, // Should be from storage
        };

        let mut data = BytesMut::new();
        data.extend_from_slice(&[MessageId::Status as u8]);
        status.encode(&mut data);
        
        if let Err(e) = context.send_message(data.freeze()).await {
            info!("[EthProtocol] Failed to send Status message: {:?}", e);
        }
    }

    async fn disconnected(&mut self, context: ProtocolContextMutRef<'_>) {
        info!("[EthProtocol] disconnected on session: {}", context.session.id);
        let _ = self.network_send.send(NetworkMessage::PeerDisconnected(context.session.id)).await;
    }

    async fn received(&mut self, context: ProtocolContextMutRef<'_>, data: Bytes) {
        if data.is_empty() {
            return;
        }

        let msg_id = data[0];
        let payload = &data[1..];

        match msg_id {
            id if id == MessageId::Status as u8 => {
                match Status::decode(&mut &payload[..]) {
                    Ok(status) => info!("[EthProtocol] Received Status from {}: {:?}", context.session.id, status),
                    Err(e) => info!("[EthProtocol] Failed to decode Status from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::Transactions as u8 => {
                match Transactions::decode(&mut &payload[..]) {
                    Ok(txs) => {
                        info!("[EthProtocol] Received {} transactions from {}", txs.0.len(), context.session.id);
                        let mut storage = self.storage.lock().await;
                        for tx in txs.0 {
                            storage.add_transaction(tx);
                        }
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode Transactions from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::NewBlock as u8 => {
                match NewBlock::decode(&mut &payload[..]) {
                    Ok(new_block) => {
                        info!("[EthProtocol] Received new block {} from {}", new_block.block.body.execution_payload.block_hash, context.session.id);
                        let mut storage = self.storage.lock().await;
                        if storage.get_block_by_hash(new_block.block.body.execution_payload.block_hash).is_none() {
                            storage.add_block(new_block.block.clone());
                            
                            // Gossip to other peers
                            let sync_send = self.sync_send.clone();
                            let block = new_block.block;
                            tokio::spawn(async move {
                                let _ = sync_send.send(SyncEvent::NewBlock(block)).await;
                            });
                        }
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode NewBlock from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::GetBlockHeaders as u8 => {
                match GetBlockHeaders::decode(&mut &payload[..]) {
                    Ok(req) => {
                        debug!("[EthProtocol] Received GetBlockHeaders from {}: {:?}", context.session.id, req);
                        let storage = self.storage.lock().await;
                        let mut headers = Vec::new();
                        let start_block = match req.block {
                            BlockHashOrNumber::Hash(h) => storage.get_block_by_hash(h).map(|b| b.body.execution_payload.block_number),
                            BlockHashOrNumber::Number(n) => Some(n),
                        };

                        if let Some(start_num) = start_block {
                            for i in 0..req.amount {
                                let num = if req.reverse {
                                    if start_num < i * (req.skip + 1) { break; }
                                    start_num - i * (req.skip + 1)
                                } else {
                                    start_num + i * (req.skip + 1)
                                };
                                if let Some(block) = storage.get_block_by_number(num) {
                                    headers.push(block.clone());
                                } else {
                                    break;
                                }
                            }
                        }

                        let resp = BlockHeaders { request_id: req.request_id, headers };
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::BlockHeaders as u8]);
                        resp.encode(&mut data);
                        let _ = context.send_message(data.freeze()).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode GetBlockHeaders from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::GetBlockBodies as u8 => {
                match GetBlockBodies::decode(&mut &payload[..]) {
                    Ok(req) => {
                        debug!("[EthProtocol] Received GetBlockBodies from {}: {:?}", context.session.id, req);
                        let storage = self.storage.lock().await;
                        let mut bodies = Vec::new();
                        for hash in req.hashes {
                            if let Some(block) = storage.get_block_by_hash(hash) {
                                bodies.push(block.clone());
                            }
                        }
                        let resp = BlockBodies { request_id: req.request_id, bodies };
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::BlockBodies as u8]);
                        resp.encode(&mut data);
                        let _ = context.send_message(data.freeze()).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode GetBlockBodies from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::GetPooledTransactions as u8 => {
                match GetPooledTransactions::decode(&mut &payload[..]) {
                    Ok(req) => {
                        debug!("[EthProtocol] Received GetPooledTransactions from {}: {:?}", context.session.id, req);
                        let storage = self.storage.lock().await;
                        let mut transactions = Vec::new();
                        for hash in req.hashes {
                            if let Some(tx) = storage.get_transaction_by_hash(hash) {
                                transactions.push(tx.clone());
                            }
                        }
                        let resp = PooledTransactions { request_id: req.request_id, transactions };
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::PooledTransactions as u8]);
                        resp.encode(&mut data);
                        let _ = context.send_message(data.freeze()).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode GetPooledTransactions from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::PooledTransactions as u8 => {
                match PooledTransactions::decode(&mut &payload[..]) {
                    Ok(txs) => {
                        info!("[EthProtocol] Received {} pooled transactions from {}", txs.transactions.len(), context.session.id);
                        let mut storage = self.storage.lock().await;
                        for tx in txs.transactions {
                            storage.add_transaction(tx);
                        }
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode PooledTransactions from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::BlockHeaders as u8 => {
                 match BlockHeaders::decode(&mut &payload[..]) {
                    Ok(headers) => {
                        info!("[EthProtocol] Received {} BlockHeaders from {}", headers.headers.len(), context.session.id);
                        let _ = self.network_send.send(NetworkMessage::SyncHeaders(context.session.id, headers)).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode BlockHeaders from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::BlockBodies as u8 => {
                 match BlockBodies::decode(&mut &payload[..]) {
                    Ok(bodies) => {
                        info!("[EthProtocol] Received {} BlockBodies from {}", bodies.bodies.len(), context.session.id);
                        let _ = self.network_send.send(NetworkMessage::SyncBodies(context.session.id, bodies)).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode BlockBodies from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::NewPooledTransactionHashes as u8 => {
                 match NewPooledTransactionHashes::decode(&mut &payload[..]) {
                    Ok(hashes) => {
                        debug!("[EthProtocol] Received {} new pooled transaction hashes from {}", hashes.0.len(), context.session.id);
                        // In a real implementation, we would check which ones we are missing and request them
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode NewPooledTransactionHashes from {}: {:?}", context.session.id, e),
                }
            }
            _ => {
                debug!("[EthProtocol] received unknown message ID {} ({} bytes) from session: {}", 
                    msg_id, data.len(), context.session.id);
            }
        }
    }
}

pub fn create_meta(
    storage: Arc<Mutex<InMemoryStorage>>, 
    sync_send: mpsc::Sender<SyncEvent>,
    network_send: mpsc::Sender<NetworkMessage>,
) -> ProtocolMeta {
    MetaBuilder::new()
        .id(ETH_PROTOCOL_ID)
        .name(|id| format!("/eth/{}", id.value()))
        .service_handle(move || ProtocolHandle::Callback(Box::new(EthProtocolHandler { 
            storage: storage.clone(),
            sync_send: sync_send.clone(),
            network_send: network_send.clone(),
        })))
        .build()
}
