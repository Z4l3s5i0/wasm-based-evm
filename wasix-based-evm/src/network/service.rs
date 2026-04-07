use crate::network::{discovery::DiscoveryService, protocol::{self, ETH_PROTOCOL_ID}, NetworkConfig};
use crate::storage::InMemoryStorage;
use std::sync::Arc;
use tokio::sync::Mutex;
use tentacle::{
    builder::ServiceBuilder,
    service::{HandshakeType, ServiceControl, ServiceError, ServiceEvent},
    traits::ServiceHandle,
    context::ServiceContext,
    multiaddr::Multiaddr,
    secio::SecioKeyPair,
};
use tracing::{info, warn};

pub struct NetworkService {
    discovery: DiscoveryService,
    p2p_control: tentacle::service::ServiceAsyncControl,
}

impl NetworkService {
    pub async fn new(
        config: NetworkConfig,
        storage: Arc<Mutex<InMemoryStorage>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let discovery = DiscoveryService::new(config.discv5_addr, config.bootnodes)?;
        
        let protocol_meta = protocol::create_meta(storage);
        
        let mut service = ServiceBuilder::default()
            .insert_protocol(protocol_meta)
            .handshake_type(HandshakeType::Secio(SecioKeyPair::secp256k1_generated()))
            .build(SimpleServiceHandle);
        
        let p2p_control = service.control().clone();
        
        // Listen on P2P address
        let listen_addr: Multiaddr = format!("/ip4/{}/tcp/{}", config.p2p_addr.ip(), config.p2p_addr.port()).parse()?;
        service.listen(listen_addr).await?;

        tokio::spawn(async move {
            service.run().await;
        });

        Ok(Self {
            discovery,
            p2p_control,
        })
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.discovery.start().await?;
        
        let mut discv5_events = self.discovery.discv5().event_stream().await
            .map_err(|e| format!("{:?}", e))?;
        
        info!("Network service running");

        loop {
            tokio::select! {
                event = discv5_events.recv() => {
                    if let Some(event) = event {
                        match event {
                            discv5::Event::Discovered(enr) => {
                                info!("Peer discovered via Discv5: {}", enr.node_id());
                                // Try to connect via P2P
                                if let Some(tcp_port) = enr.tcp4() {
                                    if let Some(ip) = enr.ip4() {
                                        let addr: Multiaddr = format!("/ip4/{}/tcp/{}", ip, tcp_port).parse().unwrap();
                                        let _ = self.p2p_control.dial(addr, tentacle::service::TargetProtocol::Single(ETH_PROTOCOL_ID)).await;
                                    }
                                }
                            }
                            _ => {}
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
        warn!("P2P Service error: {:?}", error);
    }

    async fn handle_event(&mut self, _control: &mut ServiceContext, event: ServiceEvent) {
        info!("P2P Service event: {:?}", event);
    }
}
