use std::str::FromStr;
use std::collections::HashMap;
use tentacle::SessionId;
use crate::network::{discovery::DiscoveryService, protocol::{self, ETH_PROTOCOL_ID, MessageId, NewPooledTransactionHashes}, NetworkConfig, PeerInfo};
use crate::storage::{InMemoryStorage, Transaction};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use discv5::enr::{self, Enr};
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

pub struct NetworkService {
    discovery: DiscoveryService,
    p2p_control: ServiceAsyncControl,
    rx_broadcast: mpsc::Receiver<Transaction>,
    network_recv: mpsc::Receiver<crate::network::NetworkMessage>,
    p2p_listen_addr: Arc<Mutex<Multiaddr>>,
    sessions: Arc<Mutex<HashMap<SessionId, PeerInfo>>>,
}

impl NetworkService {
    pub async fn new(
        config: NetworkConfig,
        storage: Arc<Mutex<InMemoryStorage>>,
        rx_broadcast: mpsc::Receiver<Transaction>,
        network_recv: mpsc::Receiver<crate::network::NetworkMessage>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        println!("[NetworkService] Initializing NetworkService...");
        println!("[NetworkService] Creating DiscoveryService with discv5_addr: {}, p2p_port: {}, ext_ip: {:?}", config.discv5_addr, config.p2p_addr.port(), config.ext_ip);
        let discovery = DiscoveryService::new(config.discv5_addr, config.p2p_addr.port(), config.ext_ip, config.bootnodes)?;
        println!("[NetworkService] DiscoveryService created.");
        
        let protocol_meta = protocol::create_meta(storage);
        
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        let sessions_clone = sessions.clone();

        // Listen on P2P address
        let listen_addr: Multiaddr = format!("/ip4/{}/tcp/{}", config.p2p_addr.ip(), config.p2p_addr.port()).parse()?;
        let p2p_listen_addr = Arc::new(Mutex::new(listen_addr.clone()));
        let p2p_listen_addr_clone = p2p_listen_addr.clone();

        println!("[NetworkService] Building P2P service...");
        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .handshake_type(HandshakeType::Secio(SecioKeyPair::secp256k1_generated()))
            .max_connection_number(config.max_peers)
            .build(SimpleServiceHandle { 
                sessions: sessions_clone,
                p2p_listen_addr: p2p_listen_addr_clone,
            });
        println!("[NetworkService] P2P service built.");
        
        let p2p_control = service.control().clone();
        
        // Listen on P2P address
        println!("[NetworkService] Binding P2P service to {}...", listen_addr);
        let _ = service.listen(listen_addr.clone()).await?;

        tokio::spawn(async move {
            println!("[NetworkService] P2P service background task starting...");
            service.run().await;
            println!("[NetworkService] P2P service background task finished.");
        });

        Ok(Self {
            discovery,
            p2p_control,
            rx_broadcast,
            network_recv,
            p2p_listen_addr,
            sessions,
        })
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[NetworkService] Starting NetworkService loop...");
        if let Err(e) = self.discovery.start().await {
            println!("[NetworkService] Error starting DiscoveryService: {:?}", e);
            return Err(e);
        }
        
        println!("[NetworkService] Subscribing to Discv5 events...");
        let mut discv5_events = self.discovery.event_stream().await
            .map_err(|e| format!("{:?}", e))?;
        
        println!("[NetworkService] Network service running and listening for events.");

        // Dial bootnodes on startup
        let bootnodes = self.discovery.bootnodes().await;
        println!("[NetworkService] Dialing {} bootnodes...", bootnodes.len());
        for enr in bootnodes {
            if let Some(tcp_port) = enr.tcp4() {
                if let Some(ip) = enr.ip4() {
                    let ip_str = if ip.is_unspecified() {
                        "127.0.0.1".to_string()
                    } else {
                        ip.to_string()
                    };
                    let addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip_str, tcp_port).parse().unwrap();
                    println!("[NetworkService] Attempting to dial bootnode: {}", addr);
                    let res = self.p2p_control.dial(addr.clone(), TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                    println!("[NetworkService] Bootnode dial result for {}: {:?}", addr, res);
                }
            }
        }

        loop {
            tokio::select! {
                event = discv5_events.recv() => {
                    if let Some(event) = event {
                        match event {
                            discv5::Event::Discovered(enr) => {
                                println!("[NetworkService] Peer discovered via Discv5: {}", enr.node_id());
                                // Try to connect via P2P
                                if let Some(tcp_port) = enr.tcp4() {
                                    if let Some(ip) = enr.ip4() {
                                        let ip_str = if ip.is_unspecified() {
                                            "127.0.0.1".to_string()
                                        } else {
                                            ip.to_string()
                                        };
                                        let addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip_str, tcp_port).parse().unwrap();
                                        println!("[NetworkService] Attempting to dial discovered peer: {}", addr);
                                        let res = self.p2p_control.dial(addr.clone(), TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                                        println!("[NetworkService] Dial result for {}: {:?}", addr, res);
                                    } else {
                                        println!("[NetworkService] Discovered peer {} has no IPv4", enr.node_id());
                                    }
                                } else {
                                    println!("[NetworkService] Discovered peer {} has no TCP4 port", enr.node_id());
                                }
                            }
                            _ => {
                                println!("[NetworkService] Other Discv5 event: {:?}", event);
                            }
                        }
                    }
                }
                tx = self.rx_broadcast.recv() => {
                    if let Some(tx) = tx {
                        println!("Broadcasting transaction hash: {:?}", tx.hash);
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
                            println!("Failed to broadcast transaction hash: {:?}", e);
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
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    self.discovery.find_peers().await;
                }
            }
        }
    }
}

struct SimpleServiceHandle {
    sessions: Arc<Mutex<HashMap<SessionId, PeerInfo>>>,
    p2p_listen_addr: Arc<Mutex<Multiaddr>>,
}

#[async_trait::async_trait]
impl ServiceHandle for SimpleServiceHandle {
    async fn handle_error(&mut self, _control: &mut ServiceContext, error: ServiceError) {
        println!("P2P Service error: {:?}", error);
    }

    async fn handle_event(&mut self, _control: &mut ServiceContext, event: ServiceEvent) {
        match event {
            ServiceEvent::SessionOpen { session_context } => {
                println!("P2P Service event: SessionOpen {{ id: {}, address: {}, ty: {:?} }}", 
                    session_context.id, session_context.address, session_context.ty);
                let mut sessions = self.sessions.lock().await;
                sessions.insert(session_context.id, PeerInfo {
                    id: session_context.id.to_string(),
                    addr: session_context.address.to_string(),
                    enr: None,
                });
            }
            ServiceEvent::SessionClose { session_context } => {
                println!("P2P Service event: SessionClose {{ id: {} }}", session_context.id);
                let mut sessions = self.sessions.lock().await;
                sessions.remove(&session_context.id);
            }
            ServiceEvent::ListenStarted { address } => {
                println!("P2P Service event: ListenStarted {{ address: {} }}", address);
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
                        println!("[SimpleServiceHandle] Ignoring ListenStarted with port 0 to preserve configured port.");
                    }
                }
            }
            _ => {
                println!("P2P Service event: {:?}", event);
            }
        }
    }
}
