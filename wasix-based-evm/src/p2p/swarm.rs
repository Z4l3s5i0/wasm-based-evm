use libp2p::{identify, ping, kad, swarm::{NetworkBehaviour, SwarmEvent}, Multiaddr, PeerId, Transport};
use anyhow::{Result, Context};
use crate::{info, debug};
use libp2p::futures::StreamExt;
use std::pin::Pin;
use std::future::Future;

#[derive(Clone, Copy, Debug, Default)]
struct TokioExecutor;

impl libp2p::swarm::Executor for TokioExecutor {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }
}

#[derive(NetworkBehaviour)]
pub struct EthNetworkBehaviour {
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
}

pub struct SwarmService {
    swarm: libp2p::Swarm<EthNetworkBehaviour>,
}

impl SwarmService {
    pub fn new(libp2p_keypair: libp2p::identity::Keypair, listen_port: u16, bootnodes: Vec<String>) -> Result<Self> {
        let local_peer_id = PeerId::from(libp2p_keypair.public());
        info!("[Swarm] Local PeerId: {}", local_peer_id);

        // 2. Transport setup
        let tcp_config = libp2p_tcp::Config::default().nodelay(true);
        let transport = libp2p_tcp::tokio::Transport::new(tcp_config)
            .upgrade(libp2p::core::upgrade::Version::V1Lazy)
            .authenticate(libp2p::noise::Config::new(&libp2p_keypair)?)
            .multiplex(libp2p::yamux::Config::default())
            .map(|(p, c), _| (p, libp2p::core::muxing::StreamMuxerBox::new(c)))
            .boxed();

        let kad_config = kad::Config::default();
        let store = kad::store::MemoryStore::new(local_peer_id);
        let kademlia = kad::Behaviour::with_config(local_peer_id, store, kad_config);

        let behaviour = EthNetworkBehaviour {
            identify: identify::Behaviour::new(identify::Config::new(
                "/eth/1.0.0".into(),
                libp2p_keypair.public(),
            )),
            ping: ping::Behaviour::default(),
            kademlia,
        };

        let mut swarm = libp2p::Swarm::new(
            transport,
            behaviour,
            local_peer_id,
            libp2p::swarm::Config::with_executor(TokioExecutor),
        );

        // Add bootnodes to Kademlia
        for addr_str in bootnodes {
            let addr: Multiaddr = addr_str.parse().context(format!("Failed to parse bootnode address: {}", addr_str))?;
            let peer_id = addr.iter().find_map(|p| match p {
                libp2p::multiaddr::Protocol::P2p(hash) => Some(hash),
                _ => None,
            }).context(format!("Bootnode address must include PeerId: {}", addr_str))?;
            swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
        }

        // Bootstrap Kademlia
        if let Err(e) = swarm.behaviour_mut().kademlia.bootstrap() {
            debug!("[Swarm] Kademlia bootstrap skipped: {}", e);
        }

        let listen_addr = format!("/ip4/127.0.0.1/tcp/{}", listen_port).parse()?;
        match swarm.listen_on(listen_addr) {
            Ok(_) => {}
            Err(e) => {
                return Err(e.into());
            }
        }

        Ok(Self { swarm })
    }

    pub async fn start(mut self) -> Result<()> {
        info!("[Swarm] Starting background tasks...");
        
        tokio::spawn(async move {
            loop {
                match self.swarm.next().await {
                    Some(event) => {
                        match event {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                info!("[Swarm] Listening on {}", address);
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                                info!("[Swarm] Connection established with {} via {:?}", peer_id, endpoint);
                            }
                            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                                info!("[Swarm] Connection closed with {}: {:?}", peer_id, cause);
                            }
                            SwarmEvent::IncomingConnectionError { local_addr, send_back_addr, error, .. } => {
                                debug!("[Swarm] Incoming connection error from {}: {} (local: {})", send_back_addr, error, local_addr);
                            }
                            SwarmEvent::Behaviour(EthNetworkBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result, .. })) => {
                                match result {
                                    kad::QueryResult::Bootstrap(Ok(ok)) => {
                                        debug!("[Swarm] Kademlia bootstrap progress: {:?}", ok);
                                    }
                                    kad::QueryResult::Bootstrap(Err(e)) => {
                                        debug!("[Swarm] Kademlia bootstrap error: {:?}", e);
                                    }
                                    _ => {}
                                }
                            }
                            SwarmEvent::Behaviour(EthNetworkBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
                                debug!("[Swarm] Identify received from {}: {:?}", peer_id, info);
                                for addr in info.listen_addrs {
                                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                                }
                            }
                            _ => {}
                        }
                    }
                    None => break,
                }
            }
        });

        Ok(())
    }
}
