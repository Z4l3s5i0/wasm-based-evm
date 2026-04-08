use std::str::FromStr;
use std::collections::HashMap;
use tentacle::SessionId;
use crate::network::{discovery::DiscoveryService, NetworkConfig, PeerInfo, peer_manager::PeerManager, protocol::{self, ETH_PROTOCOL_ID, MessageId, NewPooledTransactionHashes, NewBlock, Status, Transactions, GetBlockHeaders, BlockHashOrNumber, BlockHeaders, GetBlockBodies, BlockBodies, GetPooledTransactions, PooledTransactions}};
use crate::storage::{InMemoryStorage, Transaction};
use crate::network::sync::SyncEvent;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use discv5::{enr, Enr};
use alloy_primitives::U256;
use tentacle::{
    builder::{ServiceBuilder, MetaBuilder},
    service::{HandshakeType, ServiceAsyncControl, ServiceError, ServiceEvent, TargetProtocol, ProtocolHandle, ProtocolMeta},
    traits::{ServiceHandle, ServiceProtocol},
    context::{ServiceContext, ProtocolContext, ProtocolContextMutRef},
    multiaddr::Multiaddr,
    secio::SecioKeyPair,
    bytes::{Bytes, BytesMut},
};
use alloy_rlp::{Encodable, Decodable};
use crate::network::NetworkMessage;
use crate::{info, debug};

pub struct NetworkService {
    discovery: DiscoveryService,
    p2p_control: ServiceAsyncControl,
    rx_broadcast: mpsc::Receiver<Transaction>,
    network_recv: mpsc::Receiver<crate::network::NetworkMessage>,
    p2p_listen_addr: Arc<Mutex<Multiaddr>>,
    peer_manager: Arc<Mutex<PeerManager>>,
    sync_send: mpsc::Sender<SyncEvent>,
}

impl NetworkService {
    pub async fn new(
        config: NetworkConfig,
        storage: Arc<Mutex<InMemoryStorage>>,
        rx_broadcast: mpsc::Receiver<Transaction>,
        network_recv: mpsc::Receiver<crate::network::NetworkMessage>,
        sync_send: mpsc::Sender<SyncEvent>,
        network_send: mpsc::Sender<crate::network::NetworkMessage>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        debug!("[NetworkService] Initializing NetworkService...");
        debug!("[NetworkService] Creating DiscoveryService with discv5_addr: {}, p2p_port: {}, ext_ip: {:?}", config.discv5_addr, config.p2p_addr.port(), config.ext_ip);
        //2
        let discovery = DiscoveryService::new(config.discv5_addr, config.p2p_addr.port(), config.ext_ip, config.bootnodes)?;
        debug!("[NetworkService] DiscoveryService created.");
        
        let protocol_meta = create_meta(storage, sync_send.clone(), network_send);
        
        let peer_manager = Arc::new(Mutex::new(PeerManager::new()));
        let peer_manager_clone = peer_manager.clone();

        // Listen on P2P address
        let listen_addr: Multiaddr = format!("/ip4/{}/tcp/{}", config.p2p_addr.ip(), config.p2p_addr.port()).parse()?;
        let p2p_listen_addr = Arc::new(Mutex::new(listen_addr.clone()));
        let p2p_listen_addr_clone = p2p_listen_addr.clone();

        debug!("[NetworkService] Building P2P service...");
        let mut yamux_config = tentacle::yamux::config::Config::default();
        yamux_config.enable_keepalive = true;
        yamux_config.keepalive_interval = std::time::Duration::from_secs(30);
        yamux_config.connection_write_timeout = std::time::Duration::from_secs(60);

        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .handshake_type(HandshakeType::Secio(SecioKeyPair::secp256k1_generated()))
            .max_connection_number(config.max_peers)
            .yamux_config(yamux_config)
            .build(SimpleServiceHandle { 
                peer_manager: peer_manager_clone,
                p2p_listen_addr: p2p_listen_addr_clone,
                sync_send: sync_send.clone(),
            });
        debug!("[NetworkService] P2P service built.");
        
        let p2p_control = service.control().clone();
        
        // Listen on P2P address
        debug!("[NetworkService] Binding P2P service to {}...", listen_addr);
        let _ = service.listen(listen_addr.clone()).await?;

        tokio::spawn(async move {
            debug!("[NetworkService] P2P service background task starting...");
            service.run().await;
            debug!("[NetworkService] P2P service background task finished.");
        });

        Ok(Self {
            discovery,
            p2p_control,
            rx_broadcast,
            network_recv,
            p2p_listen_addr,
            peer_manager,
            sync_send,
        })
    }

    async fn dial_enr(&self, enr: Enr) {
        if let Some(tcp_port) = enr.tcp4() {
            if let Some(ip) = enr.ip4() {
                let ip_str = if ip.is_unspecified() {
                    "127.0.0.1".to_string()
                } else {
                    ip.to_string()
                };
                if let Ok(addr) = format!("/ip4/{}/tcp/{}", ip_str, tcp_port).parse::<Multiaddr>() {
                    debug!("[NetworkService] Attempting to dial ENR: {}", addr);
                    let _ = self.p2p_control.dial(addr, TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                }
            }
        }
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("[NetworkService] Starting NetworkService loop...");
        if let Err(e) = self.discovery.start().await {
            info!("[NetworkService] Error starting DiscoveryService: {:?}", e);
            return Err(e);
        }
        
        debug!("[NetworkService] Subscribing to Discv5 events...");
        let mut discv5_events = self.discovery.event_stream().await
            .map_err(|e| format!("{:?}", e))?;
        
        info!("[NetworkService] Network service running and listening for events.");

        // Dial bootnodes on startup
        let bootnodes = self.discovery.bootnodes().await;
        debug!("[NetworkService] Dialing {} bootnodes...", bootnodes.len());
        for enr in bootnodes {
            if let Some(tcp_port) = enr.tcp4() {
                if let Some(ip) = enr.ip4() {
                    let ip_str = if ip.is_unspecified() {
                        "127.0.0.1".to_string()
                    } else {
                        ip.to_string()
                    };
                    let addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip_str, tcp_port).parse().unwrap();
                    debug!("[NetworkService] Attempting to dial bootnode: {}", addr);
                    let res = self.p2p_control.dial(addr.clone(), TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                    debug!("[NetworkService] Bootnode dial result for {}: {:?}", addr, res);
                }
            }
        }

        loop {
            tokio::select! {
                event = discv5_events.recv() => {
                    debug!("[NetworkService] Received Discv5 event");
                    if let Some(event) = event {
                        match event {
                            discv5::Event::Discovered(enr) => {
                                info!("[NetworkService] Peer discovered via Discv5: {}", enr.node_id());
                                // Try to connect via P2P
                                self.dial_enr(enr).await;
                            }
                            discv5::Event::NodeInserted { node_id, .. } => {
                                info!("[NetworkService] Node inserted into DHT: {}", node_id);
                                // NodeInserted only gives node_id, not ENR.
                                // We might want to look up the ENR if we want to dial it.
                                let enr = {
                                    let discv5 = self.discovery.discv5_clone();
                                    let lock = discv5.lock().await;
                                    lock.find_enr(&node_id)
                                };
                                
                                if let Some(enr) = enr {
                                    self.dial_enr(enr).await;
                                }
                            }
                            _ => {
                                debug!("[NetworkService] Other Discv5 event: {:?}", event);
                            }
                        }
                    }
                }
                tx = self.rx_broadcast.recv() => {
                    debug!("[NetworkService] Received Transaction broadcast request");
                    if let Some(tx) = tx {
                        info!("[NetworkService] Broadcasting transaction hash: {:?}", tx.hash);
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::NewPooledTransactionHashes as u8]);
                        let msg = NewPooledTransactionHashes(vec![tx.hash]);
                        msg.encode(&mut data);
                        let msg_bytes = data.freeze();
                        
                        if let Err(e) = self.p2p_control.filter_broadcast(
                            tentacle::service::TargetSession::All,
                            ETH_PROTOCOL_ID,
                            msg_bytes
                        ).await {
                            info!("[NetworkService] Failed to broadcast transaction hash: {:?}", e);
                        }
                    }
                }
                msg = self.network_recv.recv() => {
                    debug!("[NetworkService] Received NetworkMessage");
                    if let Some(msg) = msg {
                        match msg {
                            crate::network::NetworkMessage::GetPeerCount(tx) => {
                                let len = {
                                    let pm = self.peer_manager.lock().await;
                                    pm.peer_count()
                                };
                                let _ = tx.send(len);
                            }
                            crate::network::NetworkMessage::GetPeers(tx) => {
                                let peer_infos = {
                                    let pm = self.peer_manager.lock().await;
                                    pm.get_all_peers()
                                };
                                let _ = tx.send(peer_infos);
                            }
                            crate::network::NetworkMessage::AddPeer(addr, tx) => {
                                if let Ok(enr) = Enr::from_str(&addr) {
                                     let _ = self.discovery.add_enr(enr).await;
                                     let _ = tx.send(Ok(()));
                                } else if let Ok(maddr) = addr.parse::<Multiaddr>() {
                                    let _ = self.p2p_control.dial(maddr, TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                                    let _ = tx.send(Ok(()));
                                } else {
                                    let _ = tx.send(Err("Invalid address format".to_string()));
                                }
                            }
                            crate::network::NetworkMessage::GetNodeInfo(tx) => {
                                let local_enr = self.discovery.discv5_local_enr().await;
                                let p2p_listen_addr = self.p2p_listen_addr.clone();
                                let addr = {
                                    let lock = p2p_listen_addr.lock().await;
                                    lock.to_string()
                                };
                                let node_info = crate::network::NodeInfo {
                                    enr: local_enr.to_base64(),
                                    node_id: local_enr.node_id().to_string(),
                                    listen_addresses: vec![addr],
                                };
                                let _ = tx.send(node_info);
                            }
                            crate::network::NetworkMessage::BroadcastBlock(block) => {
                                info!("[NetworkService] Broadcasting block: {}", block.body.execution_payload.block_hash);
                                let mut data = BytesMut::new();
                                data.extend_from_slice(&[MessageId::NewBlock as u8]);
                                let msg = NewBlock { 
                                    block,
                                    total_difficulty: U256::ZERO,
                                };
                                msg.encode(&mut data);
                                let msg_bytes = data.freeze();
                                
                                if let Err(e) = self.p2p_control.filter_broadcast(
                                    tentacle::service::TargetSession::All,
                                    ETH_PROTOCOL_ID,
                                    msg_bytes
                                ).await {
                                    info!("[NetworkService] Failed to broadcast block: {:?}", e);
                                }
                            }
                            crate::network::NetworkMessage::RequestHeaders { session_id, request } => {
                                debug!("[NetworkService] Sending GetBlockHeaders (id: {}) to session {}", request.request_id, session_id);
                                let mut data = BytesMut::new();
                                data.extend_from_slice(&[MessageId::GetBlockHeaders as u8]);
                                request.encode(&mut data);
                                let _ = self.p2p_control.send_message_to(session_id, ETH_PROTOCOL_ID, data.freeze()).await;
                            }
                            crate::network::NetworkMessage::RequestBodies { session_id, request } => {
                                debug!("[NetworkService] Sending GetBlockBodies (id: {}) to session {}", request.request_id, session_id);
                                let mut data = BytesMut::new();
                                data.extend_from_slice(&[MessageId::GetBlockBodies as u8]);
                                request.encode(&mut data);
                                let _ = self.p2p_control.send_message_to(session_id, ETH_PROTOCOL_ID, data.freeze()).await;
                            }
                            crate::network::NetworkMessage::SyncHeaders(session_id, headers) => {
                                let _ = self.sync_send.send(SyncEvent::Headers(session_id, headers)).await;
                            }
                            crate::network::NetworkMessage::SyncBodies(session_id, bodies) => {
                                let _ = self.sync_send.send(SyncEvent::Bodies(session_id, bodies)).await;
                            }
                            crate::network::NetworkMessage::PeerDisconnected(session_id) => {
                                let _ = self.sync_send.send(SyncEvent::PeerDisconnected(session_id)).await;
                            }
                            crate::network::NetworkMessage::ReportPeer(session_id, score) => {
                                let mut pm = self.peer_manager.lock().await;
                                pm.report_peer(&session_id, score);
                                if let Some(peer) = pm.get_peer(&session_id) {
                                    if peer.reputation < -100 {
                                        info!("[NetworkService] Disconnecting peer {} due to low reputation ({})", session_id, peer.reputation);
                                        let _ = self.p2p_control.disconnect(session_id).await;
                                    }
                                }
                            }
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                    debug!("[NetworkService] Discovery tick started");
                    self.discovery.find_peers().await;
                    debug!("[NetworkService] Discovery tick finished");
                    
                    // Periodic reputation recovery
                    debug!("[NetworkService] Periodic reputation recovery started");
                    {
                        let mut pm = self.peer_manager.lock().await;
                        pm.decay_reputation();
                    }
                    debug!("[NetworkService] Periodic reputation recovery finished");
                }
            }
        }
    }
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

struct SimpleServiceHandle {
    peer_manager: Arc<Mutex<PeerManager>>,
    p2p_listen_addr: Arc<Mutex<Multiaddr>>,
    sync_send: mpsc::Sender<SyncEvent>,
}

#[async_trait::async_trait]
impl ServiceHandle for SimpleServiceHandle {
    async fn handle_error(&mut self, _control: &mut ServiceContext, error: ServiceError) {
        debug!("[P2P Service] error: {:?}", error);
    }

    async fn handle_event(&mut self, _control: &mut ServiceContext, event: ServiceEvent) {
        match event {
            ServiceEvent::SessionOpen { session_context } => {
                info!("[P2P Service] event: SessionOpen {{ id: {}, address: {}, ty: {:?} }}", 
                    session_context.id, session_context.address, session_context.ty);
                
                let addr_str = session_context.address.to_string();
                let mut pm = self.peer_manager.lock().await;
                // Check if we are already connected to this address (best effort)
                if let Some(existing_peer) = pm.find_peer_by_addr(&addr_str) {
                    info!("[P2P Service] Already connected to {} as SessionId({}), potential duplicate session", addr_str, existing_peer.id);
                }

                pm.add_peer(session_context.id, PeerInfo {
                    id: session_context.id.to_string(),
                    addr: addr_str,
                    enr: None,
                    reputation: 0,
                });
                // Notify sync that a new peer connected
                let _ = self.sync_send.send(SyncEvent::PeerConnected(session_context.id)).await;
            }
            ServiceEvent::SessionClose { session_context } => {
                info!("[P2P Service] event: SessionClose {{ id: {} }}", session_context.id);
                let mut pm = self.peer_manager.lock().await;
                pm.remove_peer(&session_context.id);
                // Notify sync that a peer disconnected
                let _ = self.sync_send.send(SyncEvent::PeerDisconnected(session_context.id)).await;
            }
            ServiceEvent::ListenStarted { address } => {
                info!("[P2P Service] event: ListenStarted {{ address: {} }}", address);
                // On some WASIX environments, address might incorrectly report port 0.
                // We only update if port is not 0, otherwise we keep our configured address.
                if let Some(port) = address.iter().find_map(|p| {
                    if let tentacle::multiaddr::Protocol::Tcp(port) = p {
                        Some(port)
                    } else {
                        None
                    }
                }) {
                    if port != 0 {
                        let mut listen_addr = self.p2p_listen_addr.lock().await;
                        *listen_addr = address;
                    } else {
                        debug!("[P2P Service] Ignoring ListenStarted with port 0 to preserve configured port.");
                    }
                }
            }
            _ => {
                debug!("[P2P Service] event: {:?}", event);
            }
        }
    }
}