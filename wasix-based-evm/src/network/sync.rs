use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use crate::storage::{InMemoryStorage, Block};
use crate::network::NetworkMessage;
use crate::network::protocol::{GetBlockHeaders, BlockHashOrNumber, GetBlockBodies};
use crate::executor::Executor;
use std::collections::VecDeque;

pub struct SyncService {
    storage: Arc<Mutex<InMemoryStorage>>,
    network_send: mpsc::Sender<NetworkMessage>,
    sync_recv: mpsc::Receiver<SyncEvent>,
    executor: Executor,
    pending_headers: VecDeque<Block>,
}

pub enum SyncEvent {
    Headers(tentacle::SessionId, crate::network::protocol::BlockHeaders),
    Bodies(tentacle::SessionId, crate::network::protocol::BlockBodies),
    PeerConnected(tentacle::SessionId),
    NewBlock(Block),
}

impl SyncService {
    pub fn new(
        storage: Arc<Mutex<InMemoryStorage>>,
        network_send: mpsc::Sender<NetworkMessage>,
    ) -> (Self, mpsc::Sender<SyncEvent>) {
        let (sync_send, sync_recv) = mpsc::channel(100);
        (
            Self {
                storage,
                network_send,
                sync_recv,
                executor: Executor::new(),
                pending_headers: VecDeque::new(),
            },
            sync_send,
        )
    }

    pub async fn run(mut self) {
        println!("[SyncService] Starting SyncService...");
        
        loop {
            tokio::select! {
                event = self.sync_recv.recv() => {
                    if let Some(event) = event {
                        match event {
                            SyncEvent::PeerConnected(session_id) => {
                                println!("[SyncService] New peer connected: {}. Requesting headers...", session_id);
                                self.request_headers(session_id).await;
                            }
                            SyncEvent::Headers(session_id, headers) => {
                                println!("[SyncService] Received {} headers from session {}", headers.headers.len(), session_id);
                                self.handle_headers(session_id, headers).await;
                            }
                            SyncEvent::Bodies(session_id, bodies) => {
                                println!("[SyncService] Received {} bodies from session {}", bodies.bodies.len(), session_id);
                                self.handle_bodies(bodies).await;
                            }
                            SyncEvent::NewBlock(block) => {
                                println!("[SyncService] Received new block via gossip: {}. Broadcasting to other peers...", block.body.execution_payload.block_hash);
                                let _ = self.network_send.send(NetworkMessage::BroadcastBlock(block)).await;
                            }
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                    // Periodic check or maintenance
                }
            }
        }
    }

    async fn request_headers(&self, session_id: tentacle::SessionId) {
        let latest_number = {
            let storage = self.storage.lock().await;
            storage.get_latest_block_number()
        };

        let request = GetBlockHeaders {
            request_id: 1, // Should be incremented
            block: BlockHashOrNumber::Number(latest_number + 1),
            amount: 10,
            skip: 0,
            reverse: false,
        };

        if let Err(e) = self.network_send.send(NetworkMessage::RequestHeaders {
            session_id,
            request,
        }).await {
            println!("[SyncService] Failed to send RequestHeaders: {:?}", e);
        }
    }

    async fn handle_headers(&mut self, session_id: tentacle::SessionId, headers: crate::network::protocol::BlockHeaders) {
        if headers.headers.is_empty() {
            return;
        }

        let mut hashes = Vec::new();
        for header in headers.headers {
            hashes.push(header.body.execution_payload.block_hash);
            self.pending_headers.push_back(header);
        }

        let request = GetBlockBodies {
            request_id: 1,
            hashes,
        };

        if let Err(e) = self.network_send.send(NetworkMessage::RequestBodies {
            session_id,
            request,
        }).await {
            println!("[SyncService] Failed to send RequestBodies: {:?}", e);
        }
    }

    async fn handle_bodies(&mut self, bodies: crate::network::protocol::BlockBodies) {
        let mut storage = self.storage.lock().await;
        for body in bodies.bodies {
            if let Some(header) = self.pending_headers.pop_front() {
                // In a real implementation, we should match body with header via hash
                if header.body.execution_payload.block_hash == body.body.execution_payload.block_hash {
                    println!("[SyncService] Executing block {}", header.body.execution_payload.block_number);
                    // Execute block
                    let txs = body.body.execution_payload.transactions.clone();
                    match self.executor.execute_block(&mut storage, txs, body.clone()) {
                        Ok(_) => {
                            println!("[SyncService] Block {} executed successfully", body.body.execution_payload.block_number);
                            storage.add_block(body);
                        }
                        Err(e) => {
                            println!("[SyncService] Failed to execute block {}: {:?}", body.body.execution_payload.block_number, e);
                        }
                    }
                } else {
                    println!("[SyncService] Mismatched body for block");
                    self.pending_headers.push_front(header);
                    break;
                }
            }
        }
    }
}
