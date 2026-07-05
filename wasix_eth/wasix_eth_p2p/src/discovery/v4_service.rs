use crate::discovery::kbuckets::{NodeRecord, RoutingTable};
use crate::discovery::v4::{Enode, Enr, FindNode, NodeEndpoint, Packet, Ping, RawPacket};
use alloy_primitives::{B256, B512};
use alloy_rlp::Encodable;
use anyhow::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use wasix_eth_utils::identity::Identity;
use wasix_eth_utils::metrics::P2P_DISCOVERY_NODES_FOUND;
use wasix_eth_utils::{debug, error, info};

pub struct DiscoveryV4Service {
    pub(crate) local_identity: Identity,
    pub(crate) local_endpoint: NodeEndpoint,
    pub(crate) routing_table: Arc<Mutex<RoutingTable>>,
    pub(crate) socket: Arc<UdpSocket>,
    pub(crate) bootnodes: Vec<String>,
    pub(crate) bonded_peers: Arc<Mutex<HashMap<B512, (SocketAddr, Instant)>>>,
    pub(crate) pending_pings: Arc<Mutex<HashMap<B256, SocketAddr>>>,
    pub(crate) pending_find_nodes: Arc<Mutex<HashMap<B512, (SocketAddr, Instant)>>>,
    pub(crate) sessions: Arc<Mutex<HashMap<B512, SocketAddr>>>,
    pub(crate) peer_errors: Arc<Mutex<HashMap<SocketAddr, (u32, Instant)>>>,
    pub(crate) local_enr: Arc<Mutex<Enr>>,
}

impl DiscoveryV4Service {
    pub async fn new(
        local_identity: Identity,
        addr: &str,
        udp_port: u16,
        tcp_port: u16,
        bootnodes: Vec<String>,
        ext_ip: Option<std::net::IpAddr>,
    ) -> Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        let local_addr = socket.local_addr()?;
        
        let local_endpoint = NodeEndpoint {
            ip: ext_ip.unwrap_or(local_addr.ip()),
            udp_port,
            tcp_port,
        };

        let uncompressed = local_identity.keypair.verifying_key().to_encoded_point(false);
        let mut id_bytes = [0u8; 64];
        id_bytes.copy_from_slice(&uncompressed.as_bytes()[1..]);
        let local_id = B512::from(id_bytes);

        // Generate initial ENR
        let mut data = vec![
            ("id".to_string(), "v4".as_bytes().to_vec()),
            ("secp256k1".to_string(), local_identity.keypair.verifying_key().to_encoded_point(true).as_bytes().to_vec()),
        ];

        if let std::net::IpAddr::V4(ip4) = local_endpoint.ip {
            data.push(("ip".to_string(), ip4.octets().to_vec()));
        } else if let std::net::IpAddr::V6(ip6) = local_endpoint.ip {
            data.push(("ip6".to_string(), ip6.octets().to_vec()));
        }

        data.push(("udp".to_string(), udp_port.to_be_bytes().to_vec()));
        data.push(("tcp".to_string(), tcp_port.to_be_bytes().to_vec()));

        let local_enr = Enr::new(now_u64(), &local_identity.keypair, data);

        Ok(Self {
            local_identity,
            local_endpoint,
            routing_table: Arc::new(Mutex::new(RoutingTable::new(local_id))),
            socket: Arc::new(socket),
            bootnodes,
            bonded_peers: Arc::new(Mutex::new(HashMap::new())),
            pending_pings: Arc::new(Mutex::new(HashMap::new())),
            pending_find_nodes: Arc::new(Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            peer_errors: Arc::new(Mutex::new(HashMap::new())),
            local_enr: Arc::new(Mutex::new(local_enr)),
        })
    }

    pub async fn start(self: Arc<Self>) {
        let local_addr = self.socket.local_addr().unwrap();
        let port = if local_addr.port() == 0 {
            self.local_endpoint.udp_port
        } else {
            local_addr.port()
        };

        let display_addr = if let Some(ext_ip) = self.local_endpoint.ip.into() {
            format!("{}:{}", ext_ip, port)
        } else {
            format!("{}:{}", local_addr.ip(), port)
        };
        info!("[DiscoveryV4] Starting UDP service on {}", display_addr);
        
        // Initial bootstrap
        for bootnode in &self.bootnodes {
            if let Ok(enode) = Enode::from_str(bootnode) {
                let addr = SocketAddr::new(enode.ip, enode.udp_port);
                // Mark bootnodes as bonded initially so we can talk to them
                self.bonded_peers.lock().await.insert(enode.id, (addr, Instant::now()));
                let _ = self.ping_node(addr).await;
                // Add to routing table immediately if it's a bootnode
                self.add_node_to_table(enode.id, NodeEndpoint {
                    ip: enode.ip,
                    udp_port: enode.udp_port,
                    tcp_port: enode.tcp_port,
                }).await;
            } else if let Ok(addr) = bootnode.parse::<SocketAddr>() {
                // If it's just an IP:Port, we don't have the ID yet, so we can't bond it by ID.
                // We'll have to wait for the first Ping/Pong.
                let _ = self.ping_node(addr).await;
            }
        }

        let service_for_handler = self.clone();
        let handler = Arc::new(crate::discovery::v4::DiscoveryHandler::new(service_for_handler));
        
        let service = self.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 1280];
            loop {
                match service.socket.recv_from(&mut buf).await {
                    Ok((size, from)) => {
                        if let Err(_e) = handler.handle_packet(&buf[..size], from).await {
                            // handle_packet already logs
                        }
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("Operation timed out") || err_str.contains("os error 73") {
                             // This is common in wasix and usually transient, just continue
                             debug!("[DiscoveryV4] UDP receive timeout (transient)");
                             continue;
                        }
                        error!("[DiscoveryV4] UDP receive error: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        // Background loop for discovery maintenance
        let worker = Arc::new(crate::discovery::v4::DiscoveryWorker::new(self.clone()));
        worker.start().await;
    }

    pub async fn is_bonded(&self, id: B512) -> bool {
        let mut bonded = self.bonded_peers.lock().await;
        
        if let Some((_, time)) = bonded.get(&id) {
            if time.elapsed() < Duration::from_secs(24 * 3600) {
                return true;
            }
            // Expired
            bonded.remove(&id);
        }
        
        false
    }

    pub async fn ping_node(&self, addr: SocketAddr) -> Result<()> {
        let ping = Ping {
            version: 4,
            from: self.local_endpoint.clone(),
            to: NodeEndpoint {
                ip: addr.ip(),
                udp_port: addr.port(),
                tcp_port: 0, // Unknown
            },
            expiration: now_u64() + 60,
            enr_seq: Some(self.local_enr.lock().await.seq),
        };

        let raw = RawPacket::new(Packet::Ping(ping), &self.local_identity.keypair);
        self.pending_pings.lock().await.insert(raw.hash, addr);
        
        let encoded = raw.encode();
        self.socket.send_to(&encoded, addr).await?;
        Ok(())
    }

    pub async fn send_packet(&self, packet: Packet, addr: SocketAddr) -> Result<()> {
        let raw = RawPacket::new(packet, &self.local_identity.keypair);
        let encoded = raw.encode();
        self.socket.send_to(&encoded, addr).await?;
        Ok(())
    }

    pub async fn send_find_node(&self, addr: SocketAddr, target: B512) -> Result<()> {
        let find_node = FindNode {
            target,
            expiration: now_u64() + 60,
        };
        let raw = RawPacket::new(Packet::FindNode(find_node), &self.local_identity.keypair);
        self.pending_find_nodes.lock().await.insert(target, (addr, Instant::now()));
        
        let encoded = raw.encode();
        self.socket.send_to(&encoded, addr).await?;
        Ok(())
    }

    pub async fn add_node_to_table(&self, id: B512, endpoint: NodeEndpoint) {

        let mut table = self.routing_table.lock().await;
        let is_new = table.get_all_nodes().iter().all(|n| n.id != id);
        let evicted = table.add_node(id, endpoint.clone());
        drop(table);

        if let Some(evicted) = evicted {
            let addr = SocketAddr::new(evicted.endpoint.ip, evicted.endpoint.udp_port);
            info!("[DiscoveryV4] Bucket full, pinging evicted node {} to see if it's still alive", addr);
            let _ = self.ping_node(addr).await;
        } else {
            if is_new {
                P2P_DISCOVERY_NODES_FOUND.inc();
            }
            info!("[DiscoveryV4] Added/Updated node in routing table: {} (ID: {})", 
                SocketAddr::new(endpoint.ip, endpoint.udp_port), id);
        }
    }

    pub async fn get_all_nodes(&self) -> Vec<NodeRecord> {
        self.routing_table.lock().await.get_all_nodes()
    }

    pub async fn get_node_by_addr(&self, addr: SocketAddr) -> Option<NodeRecord> {
        self.routing_table.lock().await.get_all_nodes().into_iter().find(|n| {
            n.endpoint.ip == addr.ip() && (n.endpoint.tcp_port == addr.port() || n.endpoint.udp_port == addr.port())
        })
    }

    pub async fn get_local_enr_struct(&self) -> Enr {
        self.local_enr.lock().await.clone()
    }

    pub async fn get_local_enr(&self) -> Vec<u8> {
        let enr = self.local_enr.lock().await;
        let mut out = Vec::new();
        enr.encode(&mut out);
        out
    }

    pub fn identity(&self) -> &Identity {
        &self.local_identity
    }
}

pub fn now_u64() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}
