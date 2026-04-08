use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tentacle::{
    service::{ProtocolHandle, ProtocolMeta},
    traits::ServiceProtocol,
    context::{ProtocolContext, ProtocolContextMutRef},
    bytes::{Bytes, BytesMut},
    builder::MetaBuilder,
};
use alloy_rlp::{Encodable, Decodable};
use alloy_primitives::U256;
use crate::network::{
    protocol::{
        ETH_PROTOCOL_ID, MessageId, NewPooledTransactionHashes, NewBlock, Status, 
        Transactions, GetBlockHeaders, BlockHashOrNumber, BlockHeaders, 
        GetBlockBodies, BlockBodies, GetPooledTransactions, PooledTransactions
    },
    NetworkMessage,
    sync::SyncEvent,
};
use crate::storage::InMemoryStorage;
use crate::storage::types::Block;
use crate::{info, debug};

pub struct ProtocolDispatcher {
    storage: Arc<Mutex<InMemoryStorage>>,
    sync_send: mpsc::Sender<SyncEvent>,
    network_send: mpsc::Sender<NetworkMessage>,
}

impl ProtocolDispatcher {
    pub fn new(
        storage: Arc<Mutex<InMemoryStorage>>,
        sync_send: mpsc::Sender<SyncEvent>,
        network_send: mpsc::Sender<NetworkMessage>,
    ) -> Self {
        Self {
            storage,
            sync_send,
            network_send,
        }
    }

    pub async fn dispatch(&self, context: ProtocolContextMutRef<'_>, data: Bytes) {
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

                        let res = BlockHeaders { request_id: req.request_id, headers };
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::BlockHeaders as u8]);
                        res.encode(&mut data);
                        let _ = context.send_message(data.freeze()).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode GetBlockHeaders from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::GetBlockBodies as u8 => {
                match GetBlockBodies::decode(&mut &payload[..]) {
                    Ok(req) => {
                        debug!("[EthProtocol] Received GetBlockBodies from {} with {} hashes", context.session.id, req.hashes.len());
                        let storage = self.storage.lock().await;
                        let mut bodies: Vec<Block> = Vec::new();
                        for hash in req.hashes {
                            if let Some(block) = storage.get_block_by_hash(hash) {
                                bodies.push(block.clone());
                            }
                        }

                        let res = BlockBodies { request_id: req.request_id, bodies };
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::BlockBodies as u8]);
                        res.encode(&mut data);
                        let _ = context.send_message(data.freeze()).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode GetBlockBodies from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::GetPooledTransactions as u8 => {
                match GetPooledTransactions::decode(&mut &payload[..]) {
                    Ok(req) => {
                        debug!("[EthProtocol] Received GetPooledTransactions from {} with {} hashes", context.session.id, req.hashes.len());
                        let storage = self.storage.lock().await;
                        let mut txs = Vec::new();
                        for hash in req.hashes {
                            if let Some(tx) = storage.get_transaction_by_hash(hash) {
                                txs.push(tx.clone());
                            }
                        }

                        let res = PooledTransactions { request_id: req.request_id, transactions: txs };
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::PooledTransactions as u8]);
                        res.encode(&mut data);
                        let _ = context.send_message(data.freeze()).await;
                    }
                    Err(e) => info!("[EthProtocol] Failed to decode GetPooledTransactions from {}: {:?}", context.session.id, e),
                }
            }
            id if id == MessageId::PooledTransactions as u8 => {
                match PooledTransactions::decode(&mut &payload[..]) {
                    Ok(txs) => {
                        info!("[EthProtocol] Received {} PooledTransactions from {}", txs.transactions.len(), context.session.id);
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

pub struct EthProtocolHandler {
    storage: Arc<Mutex<InMemoryStorage>>,
    network_send: mpsc::Sender<NetworkMessage>,
    dispatcher: Arc<ProtocolDispatcher>,
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
            genesis_hash: alloy_primitives::B256::ZERO, // Should be from storage
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
        self.dispatcher.dispatch(context, data).await;
    }
}

pub fn create_meta(
    storage: Arc<Mutex<InMemoryStorage>>, 
    sync_send: mpsc::Sender<SyncEvent>,
    network_send: mpsc::Sender<NetworkMessage>,
) -> ProtocolMeta {
    let dispatcher = Arc::new(ProtocolDispatcher::new(
        storage.clone(),
        sync_send.clone(),
        network_send.clone(),
    ));

    MetaBuilder::new()
        .id(ETH_PROTOCOL_ID)
        .name(|id| format!("/eth/{}", id.value()))
        .service_handle(move || ProtocolHandle::Callback(Box::new(EthProtocolHandler { 
            storage: storage.clone(),
            network_send: network_send.clone(),
            dispatcher: dispatcher.clone(),
        })))
        .build()
}
