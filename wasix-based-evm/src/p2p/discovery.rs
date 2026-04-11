use discv5::{Discv5, ConfigBuilder, Event, ListenConfig};
use discv5::enr::{CombinedKey, Enr as RawEnr, NodeId};
use std::net::{SocketAddr, Ipv4Addr, Ipv6Addr, IpAddr};
use std::sync::Arc;
use crate::{info, error, debug};
use anyhow::{Result, anyhow};

#[cfg(target_os = "wasi")]
use wasix::{sock_bind, sock_open, AddressFamily, Socktype, Address, AddressV4, AddressV6};

pub struct DiscoveryService {
    discv5: Arc<Discv5>,
}

impl DiscoveryService {
    pub async fn new(
        identity_key: &CombinedKey,
        local_enr: RawEnr<CombinedKey>,
        listen_port: u16,
        bootnodes: Vec<String>,
    ) -> Result<Self> {
        #[cfg(target_os = "wasi")]
        {
            // Explicitly bind the UDP socket via wasix syscalls if possible
            // to ensure the sandbox has pre-mapped the interface.
            let (ip, port) = if let Some(ip4) = local_enr.ip4() {
                (IpAddr::V4(ip4), listen_port)
            } else if let Some(ip6) = local_enr.ip6() {
                (IpAddr::V6(ip6), listen_port)
            } else {
                (IpAddr::V4(Ipv4Addr::UNSPECIFIED), listen_port)
            };

            info!("[Discovery] [WASIX] Attempting manual bind for Discv5 to {}:{}", ip, port);
            
            // Note: discv5 handles its own socket creation, but calling wasix::sock_bind 
            // here can help trigger the host's networking bridge before the library starts.
            unsafe {
                let family = match ip {
                    IpAddr::V4(_) => AddressFamily::Inet,
                    IpAddr::V6(_) => AddressFamily::Inet6,
                };
                
                if let Ok(fd) = sock_open(family, Socktype::Datagram, 0) {
                    let addr = match ip {
                        IpAddr::V4(ip4) => {
                            let octets = ip4.octets();
                            Address::Inet(AddressV4 {
                                port: port,
                                addr: [octets[0], octets[1], octets[2], octets[3]],
                            })
                        }
                        IpAddr::V6(ip6) => {
                            let segments = ip6.segments();
                            Address::Inet6(AddressV6 {
                                port: port,
                                addr: [segments[0], segments[1], segments[2], segments[3], segments[4], segments[5], segments[6], segments[7]],
                                flowinfo: 0,
                                scope_id: 0,
                            })
                        }
                    };
                    let _ = sock_bind(fd, &addr);
                    // We don't need to keep the FD open as discv5 will open its own,
                    // but the bind call tells the host runtime to reserve the port.
                    let _ = wasix::fd_close(fd);
                }
            }
        }

        let listen_config = if let Some(ip4) = local_enr.ip4() {
            // In WASIX environments, binding to a specific IP (from --ext-ip) 
            // is often necessary for the host to bridge UDP traffic correctly.
            ListenConfig::Ipv4 {
                ip: ip4,
                port: listen_port,
            }
        } else if let Some(ip6) = local_enr.ip6() {
            ListenConfig::Ipv6 {
                ip: ip6,
                port: listen_port,
            }
        } else {
            // Default to UNSPECIFIED if no IP is set in ENR
            ListenConfig::from(SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), listen_port))
        };
        
        let config = ConfigBuilder::new(listen_config)
            .build();

        // CombinedKey doesn't implement Clone, so we have to manually "clone" it by encoding and decoding
        let encoded_key = identity_key.encode();
        // The first byte of the encoded CombinedKey is the type (0 for secp256k1)
        // If it's not 0, we check if it might be just the raw secret key (32 bytes)
        let mut secret_bytes = if encoded_key.len() == 33 && encoded_key[0] == 0 {
            encoded_key[1..].to_vec()
        } else if encoded_key.len() == 32 {
            encoded_key.to_vec()
        } else {
             return Err(anyhow!("Only secp256k1 is supported for cloning. Encoded length: {}, first byte: {}", encoded_key.len(), encoded_key[0]));
        };
        let cloned_key = CombinedKey::secp256k1_from_bytes(&mut secret_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to clone identity key: {:?}", e))?;

        let mut discv5 = Discv5::new(local_enr, cloned_key, config)
            .map_err(|e| anyhow::anyhow!("Failed to initialize Discv5: {}", e))?;

        for enr_str in bootnodes {
            match enr_str.parse::<RawEnr<CombinedKey>>() {
                Ok(enr) => {
                    if let Err(e) = discv5.add_enr(enr) {
                        error!("[Discovery] Failed to add bootnode ENR: {}", e);
                    }
                }
                Err(e) => {
                    error!("[Discovery] Failed to parse bootnode ENR '{}': {}", enr_str, e);
                }
            }
        }

        // Start the service before wrapping it in Arc
        discv5.start().await
            .map_err(|e| anyhow::anyhow!("Failed to start Discv5: {}", e))?;
        
        Ok(Self { discv5: Arc::new(discv5) })
    }

    pub async fn start(&self) -> Result<()> {
        info!("[Discovery] Starting background tasks...");
        
        let mut event_stream = self.discv5.event_stream().await
            .map_err(|e| anyhow::anyhow!("Failed to get Discv5 event stream: {}", e))?;

        // Active discovery loop
        let discv5_clone = Arc::clone(&self.discv5);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                debug!("[Discovery] Triggering random DHT query...");
                let target_node = NodeId::random();
                let found_nodes = discv5_clone.find_node(target_node).await;
                match found_nodes {
                    Ok(nodes) => {
                        if !nodes.is_empty() {
                            info!("[Discovery] DHT query found {} nodes", nodes.len());
                        }
                    }
                    Err(e) => debug!("[Discovery] DHT query failed: {}", e),
                }
            }
        });

        // Event handler loop
        tokio::spawn(async move {
            while let Some(event) = event_stream.recv().await {
                match event {
                    Event::Discovered(enr) => {
                        info!("[Discovery] Peer discovered: {}", enr);
                    }
                    Event::NodeInserted { node_id, replaced } => {
                        debug!("[Discovery] Node inserted: {} (replaced: {:?})", node_id, replaced);
                    }
                    Event::SocketUpdated(addr) => {
                        info!("[Discovery] External socket updated: {}", addr);
                        // Updating the ENR is usually handled internally by discv5 
                        // if enr_update is enabled (which is default).
                        // We just log it here.
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    pub fn local_enr(&self) -> RawEnr<CombinedKey> {
        self.discv5.local_enr()
    }
}
