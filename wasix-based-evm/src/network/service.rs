use std::str::FromStr;
use crate::network::{discovery::DiscoveryService, protocol::{self, ETH_PROTOCOL_ID, MessageId, Transactions}, NetworkConfig};
use crate::storage::{InMemoryStorage, Transaction};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
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
    p2p_listen_addr: Multiaddr,
}

impl NetworkService {
    pub async fn new(
        config: NetworkConfig,
        storage: Arc<Mutex<InMemoryStorage>>,
        rx_broadcast: mpsc::Receiver<Transaction>,
        network_recv: mpsc::Receiver<crate::network::NetworkMessage>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        println!("[NetworkService] Initializing NetworkService...");
        println!("[NetworkService] Creating DiscoveryService with discv5_addr: {}", config.discv5_addr);
        let discovery = DiscoveryService::new(config.discv5_addr, config.bootnodes)?;
        println!("[NetworkService] DiscoveryService created.");
        
        let protocol_meta = protocol::create_meta(storage);
        
        println!("[NetworkService] Building P2P service...");
        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .handshake_type(HandshakeType::Secio(SecioKeyPair::secp256k1_generated()))
            .build(SimpleServiceHandle);
        println!("[NetworkService] P2P service built.");
        
        let p2p_control = service.control().clone();
        
        // Listen on P2P address
        let listen_addr: Multiaddr = format!("/ip4/{}/tcp/{}", config.p2p_addr.ip(), config.p2p_addr.port()).parse()?;
        println!("[NetworkService] Binding P2P service to {}...", listen_addr);
        service.listen(listen_addr.clone()).await?;
        println!("[NetworkService] P2P service bound and listening.");

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
            p2p_listen_addr: listen_addr,
        })
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("[NetworkService] Starting NetworkService loop...");
        if let Err(e) = self.discovery.start().await {
            println!("[NetworkService] Error starting DiscoveryService: {:?}", e);
            return Err(e);
        }
        
        println!("[NetworkService] Subscribing to Discv5 events...");
        let mut discv5_events = self.discovery.discv5().event_stream().await
            .map_err(|e| format!("{:?}", e))?;
        
        println!("[NetworkService] Network service running and listening for events.");

        loop {
            tokio::select! {
                event = discv5_events.recv() => {
                    if let Some(event) = event {
                        match event {
                            discv5::Event::Discovered(enr) => {
                                println!("Peer discovered via Discv5: {}", enr.node_id());
                                // Try to connect via P2P
                                if let Some(tcp_port) = enr.tcp4() {
                                    if let Some(ip) = enr.ip4() {
                                        let addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip, tcp_port).parse().unwrap();
                                        let _ = self.p2p_control.dial(addr, TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                tx = self.rx_broadcast.recv() => {
                    if let Some(tx) = tx {
                        println!("Broadcasting transaction: {:?}", tx.hash);
                        let mut data = BytesMut::new();
                        data.extend_from_slice(&[MessageId::Transactions as u8]);
                        let msg = Transactions(vec![tx]);
                        msg.encode(&mut data);
                        let msg_bytes = data.freeze();
                        
                        if let Err(e) = self.p2p_control.filter_broadcast(
                            tentacle::service::TargetSession::All,
                            ETH_PROTOCOL_ID,
                            msg_bytes
                        ).await {
                            println!("Failed to broadcast transaction: {:?}", e);
                        }
                    }
                }
                msg = self.network_recv.recv() => {
                    if let Some(msg) = msg {
                        match msg {
                            crate::network::NetworkMessage::GetPeerCount(tx) => {
                                let _ = tx.send(0); // TODO: implement in tentacle or track sessions
                            }
                            crate::network::NetworkMessage::GetPeers(tx) => {
                                let _ = tx.send(vec![]); // TODO: implement
                            }
                            crate::network::NetworkMessage::AddPeer(addr, tx) => {
                                if let Ok(enr) = Enr::<enr::CombinedKey>::from_str(&addr) {
                                     let _ = self.discovery.discv5().add_enr(enr);
                                     let _ = tx.send(Ok(()));
                                } else if let Ok(maddr) = addr.parse::<Multiaddr>() {
                                    let _ = self.p2p_control.dial(maddr, TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                                    let _ = tx.send(Ok(()));
                                } else {
                                    let _ = tx.send(Err("Invalid address format".to_string()));
                                }
                            }
                            crate::network::NetworkMessage::GetNodeInfo(tx) => {
                                let node_info = crate::network::NodeInfo {
                                    enr: self.discovery.discv5().local_enr().to_base64(),
                                    node_id: self.discovery.discv5().local_enr().node_id().to_string(),
                                    listen_addresses: vec![self.p2p_listen_addr.to_string()],
                                };
                                let _ = tx.send(node_info);
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

struct SimpleServiceHandle;

#[async_trait::async_trait]
impl ServiceHandle for SimpleServiceHandle {
    async fn handle_error(&mut self, _control: &mut ServiceContext, error: ServiceError) {
        println!("P2P Service error: {:?}", error);
    }

    async fn handle_event(&mut self, _control: &mut ServiceContext, event: ServiceEvent) {
        println!("P2P Service event: {:?}", event);
    }
}
