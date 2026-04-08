use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use crate::storage::{InMemoryStorage, Block};
use crate::network::NetworkMessage;
use crate::network::protocol::{GetBlockHeaders, BlockHashOrNumber, GetBlockBodies};
use crate::executor::Executor;
use std::collections::{VecDeque, HashMap};
use std::time::{Instant, Duration};
use crate::{info, debug};

pub struct SyncService {
    storage: Arc<Mutex<InMemoryStorage>>,
    network_send: mpsc::Sender<NetworkMessage>,
    sync_recv: mpsc::Receiver<SyncEvent>,
    executor: Executor,
    pending_headers: VecDeque<Block>,
    pending_requests: HashMap<u64, (tentacle::SessionId, Instant, RequestType)>,
    next_request_id: u64,
}

enum RequestType {
    Headers,
    Bodies,
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
                pending_requests: HashMap::new(),
                next_request_id: 1,
            },
            sync_send,
        )
    }

    pub async fn run(mut self) {
        info!("[SyncService] Starting SyncService...");
        
        loop {
            tokio::select! {
                event = self.sync_recv.recv() => {
                    if let Some(event) = event {
                        match event {
                            SyncEvent::PeerConnected(session_id) => {
                                debug!("[SyncService] New peer connected: {}. Requesting headers...", session_id);
                                self.request_headers(session_id).await;
                            }
                            SyncEvent::Headers(session_id, headers) => {
                                info!("[SyncService] Received {} headers from session {}", headers.headers.len(), session_id);
                                self.pending_requests.remove(&headers.request_id);
                                self.handle_headers(session_id, headers).await;
                            }
                            SyncEvent::Bodies(session_id, bodies) => {
                                info!("[SyncService] Received {} bodies from session {}", bodies.bodies.len(), session_id);
                                self.pending_requests.remove(&bodies.request_id);
                                self.handle_bodies(session_id, bodies).await;
                            }
                            SyncEvent::NewBlock(block) => {
                                info!("[SyncService] Received new block via gossip: {}. Broadcasting to other peers...", block.body.execution_payload.block_hash);
                                let _ = self.network_send.send(NetworkMessage::BroadcastBlock(block)).await;
                            }
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                    self.check_timeouts().await;
                    self.periodic_reputation_adjustment().await;
                }
            }
        }
    }

    async fn check_timeouts(&mut self) {
        let now = Instant::now();
        let timeout_duration = Duration::from_secs(30);
        let mut timed_out = Vec::new();

        for (id, (session_id, start_time, _req_type)) in &self.pending_requests {
            if now.duration_since(*start_time) > timeout_duration {
                timed_out.push((*id, *session_id));
            }
        }

        for (id, session_id) in timed_out {
            info!("[SyncService] Request {} to session {} timed out. Penalizing peer.", id, session_id);
            self.pending_requests.remove(&id);
            // Relaxed penalty for timeout
            let _ = self.network_send.send(NetworkMessage::ReportPeer(session_id, -5)).await;
        }
    }

    async fn periodic_reputation_adjustment(&self) {
        // Broadcast a small reputation boost to all active peers every cycle
        // This allows peers to recover over time.
        // We'll use a special SessionId or handle it in NetworkService.
        // For now, let's just send a "heartbeat" or similar if needed, 
        // but it's easier to implement recovery in NetworkService directly.
    }

    async fn request_headers(&mut self, session_id: tentacle::SessionId) {
        let latest_number = {
            let storage = self.storage.lock().await;
            storage.get_latest_block_number()
        };

        let request_id = self.next_request_id;
        self.next_request_id += 1;

        let request = GetBlockHeaders {
            request_id,
            block: BlockHashOrNumber::Number(latest_number + 1),
            amount: 10,
            skip: 0,
            reverse: false,
        };

        self.pending_requests.insert(request_id, (session_id, Instant::now(), RequestType::Headers));

        if let Err(e) = self.network_send.send(NetworkMessage::RequestHeaders {
            session_id,
            request,
        }).await {
            info!("[SyncService] Failed to send RequestHeaders: {:?}", e);
            self.pending_requests.remove(&request_id);
        }
    }

    async fn handle_headers(&mut self, session_id: tentacle::SessionId, headers: crate::network::protocol::BlockHeaders) {
        if headers.headers.is_empty() {
            debug!("[SyncService] Received 0 headers from session {}", session_id);
            return;
        }

        let mut hashes = Vec::new();
        for header in headers.headers {
            hashes.push(header.body.execution_payload.block_hash);
            self.pending_headers.push_back(header);
        }

        let request_id = self.next_request_id;
        self.next_request_id += 1;

        let request = GetBlockBodies {
            request_id,
            hashes,
        };

        self.pending_requests.insert(request_id, (session_id, Instant::now(), RequestType::Bodies));

        if let Err(e) = self.network_send.send(NetworkMessage::RequestBodies {
            session_id,
            request,
        }).await {
            info!("[SyncService] Failed to send RequestBodies: {:?}", e);
            self.pending_requests.remove(&request_id);
        }
    }

    async fn handle_bodies(&mut self, session_id: tentacle::SessionId, bodies: crate::network::protocol::BlockBodies) {
        let mut storage = self.storage.lock().await;
        for body in bodies.bodies {
            if let Some(header) = self.pending_headers.pop_front() {
                // In a real implementation, we should match body with header via hash
                if header.body.execution_payload.block_hash == body.body.execution_payload.block_hash {
                    debug!("[SyncService] Executing block {}", header.body.execution_payload.block_number);
                    // Execute block
                    let txs = body.body.execution_payload.transactions.clone();
                    match self.executor.execute_block(&mut storage, txs, body.clone()) {
                        Ok(_) => {
                            info!("[SyncService] Block {} executed successfully", body.body.execution_payload.block_number);
                            storage.add_block(body);
                        }
                        Err(e) => {
                            info!("[SyncService] Failed to execute block {}: {:?}. Penalizing peer.", body.body.execution_payload.block_number, e);
                            let _ = self.network_send.send(NetworkMessage::ReportPeer(session_id, -50)).await;
                        }
                    }
                } else {
                    debug!("[SyncService] Mismatched body for block. Penalizing peer.");
                    let _ = self.network_send.send(NetworkMessage::ReportPeer(session_id, -20)).await;
                    self.pending_headers.push_front(header);
                    break;
                }
            }
        }
    }
}
