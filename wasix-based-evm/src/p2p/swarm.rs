use libp2p::{identify, ping, swarm::{NetworkBehaviour, SwarmEvent}, Multiaddr, PeerId};
use discv5::enr::{CombinedKey, Enr as RawEnr, EnrPublicKey};
use anyhow::{Result, anyhow};
use std::net::IpAddr;
use crate::{info, debug};
use libp2p::futures::StreamExt;

#[derive(NetworkBehaviour)]
pub struct EthNetworkBehaviour {
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    // Future: pub eth_protocol: MyEthHandler,
}

pub struct SwarmService {
    swarm: libp2p::Swarm<EthNetworkBehaviour>,
}

impl SwarmService {
    pub fn new(keypair: &CombinedKey, listen_port: u16) -> Result<Self> {
        // 1. Setup Identity (convert discv5 CombinedKey to libp2p Keypair)
        // CombinedKey is an enum, we need to extract the secp256k1 key if it exists
        // Based on discv5 0.10.4, we can use the following to get the secret bytes:
        
        let bytes = keypair.encode(); 
        // For secp256k1, the first byte is often 0 (identifier), followed by 32 bytes of secret key.
        let mut secret_scalar = [0u8; 32];
        if bytes.len() == 33 && bytes[0] == 0 {
            secret_scalar.copy_from_slice(&bytes[1..33]);
        } else if bytes.len() == 32 {
            secret_scalar.copy_from_slice(&bytes[0..32]);
        } else {
            return Err(anyhow!("Only secp256k1 is supported. Encoded length: {}, first byte: {}", bytes.len(), bytes[0]));
        }
        
        let libp2p_keypair = libp2p::identity::Keypair::from(
            libp2p::identity::secp256k1::Keypair::from(
                libp2p::identity::secp256k1::SecretKey::try_from_bytes(&mut secret_scalar)
                    .map_err(|e| anyhow!("Invalid secp256k1 secret key: {}", e))?
            )
        );

        let local_peer_id = PeerId::from(libp2p_keypair.public());
        info!("[Swarm] Local PeerId: {}", local_peer_id);

        // 2. Transport setup
        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(libp2p_keypair)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )?
            .with_behaviour(|key| {
                Ok(EthNetworkBehaviour {
                    identify: identify::Behaviour::new(identify::Config::new(
                        "/eth/1.0.0".into(),
                        key.public(),
                    )),
                    ping: ping::Behaviour::default(),
                })
            })?
            .build();

        let listen_addr = format!("/ip4/0.0.0.0/tcp/{}", listen_port).parse()?;
        swarm.listen_on(listen_addr)?;

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

/// Bridge to convert Discv5 ENR to libp2p Multiaddr
pub fn enr_to_multiaddr(enr: &RawEnr<CombinedKey>) -> Result<Multiaddr> {
    let ip = enr.ip4().map(IpAddr::V4)
        .or_else(|| enr.ip6().map(IpAddr::V6))
        .ok_or_else(|| anyhow!("ENR has no IP address"))?;
    
    let tcp_port = enr.tcp4()
        .or_else(|| enr.tcp6())
        .ok_or_else(|| anyhow!("ENR has no TCP port"))?;

    let multiaddr = match ip {
        IpAddr::V4(ip4) => format!("/ip4/{}/tcp/{}", ip4, tcp_port).parse()?,
        IpAddr::V6(ip6) => format!("/ip6/{}/tcp/{}", ip6, tcp_port).parse()?,
    };

    Ok(multiaddr)
}

/// Extract PeerId from ENR (secp256k1)
pub fn enr_to_peer_id(enr: &RawEnr<CombinedKey>) -> Result<PeerId> {
    let public_key = enr.public_key();
    // discv5 CombinedKey public key to libp2p public key
    // This depends on the specific libp2p version and how it handles secp256k1
    // In libp2p 0.52+, we can use the following:
    
    let bytes = public_key.encode(); // This usually returns the uncompressed secp256k1 bytes (65 bytes)
    
    let libp2p_pk = libp2p::identity::secp256k1::PublicKey::try_from_bytes(&bytes)
        .map_err(|e| anyhow!("Failed to parse secp256k1 public key: {}", e))?;
    
    Ok(PeerId::from_public_key(&libp2p::identity::PublicKey::from(libp2p_pk)))
}
