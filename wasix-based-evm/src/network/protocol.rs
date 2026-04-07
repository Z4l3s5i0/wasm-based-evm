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
use crate::storage::{InMemoryStorage, Transaction};
use tracing::{info, error};
use alloy_rlp::{RlpEncodable, RlpDecodable, Encodable, Decodable};
use alloy_primitives::{B256, U256};

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
    pub block: crate::storage::Block,
    pub total_difficulty: U256,
}

struct EthProtocolHandler {
    storage: Arc<Mutex<InMemoryStorage>>,
}

#[async_trait::async_trait]
impl ServiceProtocol for EthProtocolHandler {
    async fn init(&mut self, context: &mut ProtocolContext) {
        info!("EthProtocol initiated on protocol: {}", context.proto_id);
    }

    async fn connected(&mut self, context: ProtocolContextMutRef<'_>, version: &str) {
        info!("EthProtocol connected on session: {}, version: {}", context.session.id, version);
        
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
            error!("Failed to send Status message: {:?}", e);
        }
    }

    async fn disconnected(&mut self, context: ProtocolContextMutRef<'_>) {
        info!("EthProtocol disconnected on session: {}", context.session.id);
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
                    Ok(status) => info!("Received Status from {}: {:?}", context.session.id, status),
                    Err(e) => error!("Failed to decode Status from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::Transactions as u8 => {
                match Transactions::decode(&mut &payload[..]) {
                    Ok(txs) => {
                        info!("Received {} transactions from {}", txs.0.len(), context.session.id);
                        let mut storage = self.storage.lock().await;
                        for tx in txs.0 {
                            storage.add_transaction(tx);
                        }
                    }
                    Err(e) => error!("Failed to decode Transactions from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::NewBlock as u8 => {
                match NewBlock::decode(&mut &payload[..]) {
                    Ok(new_block) => {
                        info!("Received new block {} from {}", new_block.block.body.execution_payload.block_hash, context.session.id);
                        let mut storage = self.storage.lock().await;
                        storage.add_block(new_block.block);
                    }
                    Err(e) => error!("Failed to decode NewBlock from {}: {:?}", context.session.id, e),
                }
            }
            _ => {
                info!("EthProtocol received unknown message ID {} ({} bytes) from session: {}", 
                    msg_id, data.len(), context.session.id);
            }
        }
    }
}

pub fn create_meta(storage: Arc<Mutex<InMemoryStorage>>) -> ProtocolMeta {
    MetaBuilder::new()
        .id(ETH_PROTOCOL_ID)
        .name(|id| format!("/eth/{}", id.value()))
        .service_handle(move || ProtocolHandle::Callback(Box::new(EthProtocolHandler { storage: storage.clone() })))
        .build()
}
