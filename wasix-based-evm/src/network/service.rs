use std::str::FromStr;
use std::collections::HashMap;
use tentacle::SessionId;
use crate::network::{discovery::DiscoveryService, NetworkConfig, PeerInfo, peer_manager::{PeerManager, PeerManagerEvent}, protocol_handler};
use crate::storage::{InMemoryStorage, Transaction};
use crate::network::sync::SyncEvent;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use discv5::{enr, Enr};
use alloy_primitives::U256;
use tentacle::{
    builder::ServiceBuilder,
    service::{HandshakeType, ServiceAsyncControl, ServiceError, ServiceEvent, TargetProtocol},
    traits::ServiceHandle,
    context::ServiceContext,
    multiaddr::Multiaddr,
    secio::SecioKeyPair,
};
use tentacle::bytes::BytesMut;
use alloy_rlp::Encodable;
use crate::network::protocol::{ETH_PROTOCOL_ID, MessageId, NewBlock, GetBlockHeaders, BlockHashOrNumber, BlockHeaders, GetBlockBodies, BlockBodies, GetPooledTransactions, PooledTransactions, NewPooledTransactionHashes};
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
        
        let protocol_meta = protocol_handler::create_meta(storage, sync_send.clone(), network_send);
        
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
                    let _ = self.p2p_control.dial(addr, TargetProtocol::Single(crate::network::protocol::ETH_PROTOCOL_ID)).await;
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
                let ev = {
                    let mut pm = self.peer_manager.lock().await;
                    pm.on_session_open(session_context.id, addr_str)
                };
                match ev {
                    PeerManagerEvent::Connected(id) => {
                        // Notify sync that a new peer connected
                        let _ = self.sync_send.send(SyncEvent::PeerConnected(id)).await;
                    }
                    _ => {}
                }
            }
            ServiceEvent::SessionClose { session_context } => {
                info!("[P2P Service] event: SessionClose {{ id: {} }}", session_context.id);
                let ev = {
                    let mut pm = self.peer_manager.lock().await;
                    pm.on_session_close(session_context.id)
                };
                if let PeerManagerEvent::Disconnected(id) = ev {
                    // Notify sync that a peer disconnected
                    let _ = self.sync_send.send(SyncEvent::PeerDisconnected(id)).await;
                }
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