use std::str::FromStr;
use std::collections::HashMap;
use tentacle::SessionId;
use crate::network::{discovery::DiscoveryService, protocol::{self, ETH_PROTOCOL_ID, MessageId, NewPooledTransactionHashes, NewBlock, GetBlockHeaders, GetBlockBodies, BlockHashOrNumber}, NetworkConfig, PeerInfo};
use crate::storage::{InMemoryStorage, Transaction};
use crate::network::sync::SyncEvent;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use discv5::enr::{self, Enr};
use alloy_primitives::U256;
use tentacle::{
    builder::ServiceBuilder,
    service::{HandshakeType, ServiceAsyncControl, ServiceError, ServiceEvent, TargetProtocol},
    traits::ServiceHandle,
    context::ServiceContext,
    multiaddr::Multiaddr,
    secio::SecioKeyPair,
    bytes::BytesMut,
};
use alloy_rlp::Encodable;
use crate::{info, debug};

pub struct NetworkService {
    discovery: DiscoveryService,
    p2p_control: ServiceAsyncControl,
    rx_broadcast: mpsc::Receiver<Transaction>,
    network_recv: mpsc::Receiver<crate::network::NetworkMessage>,
    p2p_listen_addr: Arc<Mutex<Multiaddr>>,
    sessions: Arc<Mutex<HashMap<SessionId, PeerInfo>>>,
    sync_send: mpsc::Sender<SyncEvent>,
}

impl NetworkService {
    pub async fn new(
        config: NetworkConfig,
        storage: Arc<Mutex<InMemoryStorage>>,
        rx_broadcast: mpsc::Receiver<Transaction>,
        network_recv: mpsc::Receiver<crate::network::NetworkMessage>,
        sync_send: mpsc::Sender<SyncEvent>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        debug!("[NetworkService] Initializing NetworkService...");
        debug!("[NetworkService] Creating DiscoveryService with discv5_addr: {}, p2p_port: {}, ext_ip: {:?}", config.discv5_addr, config.p2p_addr.port(), config.ext_ip);
        let discovery = DiscoveryService::new(config.discv5_addr, config.p2p_addr.port(), config.ext_ip, config.bootnodes)?;
        debug!("[NetworkService] DiscoveryService created.");
        
        let protocol_meta = protocol::create_meta(storage, sync_send.clone());
        
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let sessions_clone = sessions.clone();

        // Listen on P2P address
        let listen_addr: Multiaddr = format!("/ip4/{}/tcp/{}", config.p2p_addr.ip(), config.p2p_addr.port()).parse()?;
        let p2p_listen_addr = Arc::new(Mutex::new(listen_addr.clone()));
        let p2p_listen_addr_clone = p2p_listen_addr.clone();

        debug!("[NetworkService] Building P2P service...");
        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .handshake_type(HandshakeType::Secio(SecioKeyPair::secp256k1_generated()))
            .max_connection_number(config.max_peers)
            .build(SimpleServiceHandle { 
                sessions: sessions_clone,
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
            sessions,
            sync_send,
        })
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
                    if let Some(event) = event {
                        match event {
                            discv5::Event::Discovered(enr) => {
                                info!("[NetworkService] Peer discovered via Discv5: {}", enr.node_id());
                                // Try to connect via P2P
                                if let Some(tcp_port) = enr.tcp4() {
                                    if let Some(ip) = enr.ip4() {
                                        let ip_str = if ip.is_unspecified() {
                                            "127.0.0.1".to_string()
                                        } else {
                                            ip.to_string()
                                        };
                                        let addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip_str, tcp_port).parse().unwrap();
                                        debug!("[NetworkService] Attempting to dial discovered peer: {}", addr);
                                        let res = self.p2p_control.dial(addr.clone(), TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                                        debug!("[NetworkService] Dial result for {}: {:?}", addr, res);
                                    } else {
                                        debug!("[NetworkService] Discovered peer {} has no IPv4", enr.node_id());
                                    }
                                } else {
                                    debug!("[NetworkService] Discovered peer {} has no TCP4 port", enr.node_id());
                                }
                            }
                            _ => {
                                debug!("[NetworkService] Other Discv5 event: {:?}", event);
                            }
                        }
                    }
                }
                tx = self.rx_broadcast.recv() => {
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
                    if let Some(msg) = msg {
                        match msg {
                            crate::network::NetworkMessage::GetPeerCount(tx) => {
                                let sessions = self.sessions.clone();
                                tokio::spawn(async move {
                                    let sessions_map = sessions.lock().await;
                                    let _ = tx.send(sessions_map.len());
                                });
                            }
                            crate::network::NetworkMessage::GetPeers(tx) => {
                                let sessions = self.sessions.clone();
                                tokio::spawn(async move {
                                    let sessions_map = sessions.lock().await;
                                    let peer_infos: Vec<PeerInfo> = sessions_map.values().cloned().collect();
                                    let _ = tx.send(peer_infos);
                                });
                            }
                            crate::network::NetworkMessage::AddPeer(addr, tx) => {
                                if let Ok(enr) = Enr::<enr::CombinedKey>::from_str(&addr) {
                                     let discovery = self.discovery.discv5_clone();
                                     tokio::spawn(async move {
                                         let discv5 = discovery.lock().await;
                                         let _ = discv5.add_enr(enr);
                                         let _ = tx.send(Ok(()));
                                     });
                                } else if let Ok(maddr) = addr.parse::<Multiaddr>() {
                                    let _ = self.p2p_control.dial(maddr, TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                                    let _ = tx.send(Ok(()));
                                } else {
                                    let _ = tx.send(Err("Invalid address format".to_string()));
                                }
                            }
                            crate::network::NetworkMessage::GetNodeInfo(tx) => {
                                let discovery = self.discovery.discv5_clone();
                                let p2p_listen_addr = self.p2p_listen_addr.clone();
                                tokio::spawn(async move {
                                    let addr = p2p_listen_addr.lock().await;
                                    let node_info = {
                                        let discv5 = discovery.lock().await;
                                        crate::network::NodeInfo {
                                            enr: discv5.local_enr().to_base64(),
                                            node_id: discv5.local_enr().node_id().to_string(),
                                            listen_addresses: vec![addr.to_string()],
                                        }
                                    };
                                    let _ = tx.send(node_info);
                                });
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
                                let mut data = BytesMut::new();
                                data.extend_from_slice(&[MessageId::GetBlockHeaders as u8]);
                                request.encode(&mut data);
                                let _ = self.p2p_control.send_message_to(session_id, ETH_PROTOCOL_ID, data.freeze()).await;
                            }
                            crate::network::NetworkMessage::RequestBodies { session_id, request } => {
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
                            crate::network::NetworkMessage::ReportPeer(session_id, score) => {
                                let mut sessions = self.sessions.lock().await;
                                if let Some(peer) = sessions.get_mut(&session_id) {
                                    peer.reputation += score;
                                    debug!("[NetworkService] Reported peer {}: score changed by {}, new score: {}", session_id, score, peer.reputation);
                                    if peer.reputation < -100 {
                                        info!("[NetworkService] Disconnecting peer {} due to low reputation ({})", session_id, peer.reputation);
                                        let _ = self.p2p_control.disconnect(session_id).await;
                                        // Sessions is removed on SessionClose event.
                                    }
                                }
                            }
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    self.discovery.find_peers().await;
                    
                    // Periodic reputation recovery
                    let mut sessions = self.sessions.lock().await;
                    for peer in sessions.values_mut() {
                        if peer.reputation < 0 {
                            peer.reputation += 1;
                        } else if peer.reputation > 0 {
                            peer.reputation -= 1; // Decay positive reputation towards 0 too, or keep it?
                            // Usually we want to reward long-term good behavior but not let it grow indefinitely.
                        }
                    }
                }
            }
        }
    }
}

struct SimpleServiceHandle {
    sessions: Arc<Mutex<HashMap<SessionId, PeerInfo>>>,
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
                let mut sessions = self.sessions.lock().await;
                sessions.insert(session_context.id, PeerInfo {
                    id: session_context.id.to_string(),
                    addr: session_context.address.to_string(),
                    enr: None,
                    reputation: 0,
                });
                // Notify sync that a new peer connected
                let _ = self.sync_send.send(SyncEvent::PeerConnected(session_context.id)).await;
            }
            ServiceEvent::SessionClose { session_context } => {
                info!("[P2P Service] event: SessionClose {{ id: {} }}", session_context.id);
                let mut sessions = self.sessions.lock().await;
                sessions.remove(&session_context.id);
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
