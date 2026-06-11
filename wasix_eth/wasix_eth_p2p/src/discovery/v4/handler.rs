use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use alloy_primitives::{B256, B512};
use wasix_eth_utils::{debug, info, warn};
use crate::discovery::v4::{Packet, Ping, Pong, FindNode, Neighbors, Neighbor, NodeEndpoint, RawPacket, DecodeError, ENRRequest, ENRResponse};
use crate::discovery::v4_service::{DiscoveryV4Service, now_u64};
use std::time::{Duration, Instant};

pub struct DiscoveryHandler {
    service: Arc<DiscoveryV4Service>,
}

impl DiscoveryHandler {
    pub fn new(service: Arc<DiscoveryV4Service>) -> Self {
        Self { service }
    }

    pub async fn handle_packet(&self, data: &[u8], from: SocketAddr) -> Result<()> {
        let raw = match RawPacket::decode(data) {
            Ok(r) => r,
            Err(e) => {
                // Ignore gracefully if it's too short or invalid for v4
                // Hive tests v5 and it might send packets that we can't decode as v4
                if !matches!(e, DecodeError::TooShort | DecodeError::InvalidPacketType(_)) {
                    self.report_peer_error(from, e, data).await;
                }
                return Ok(());
            }
        };

        let remote_id = match raw.recover_public_key() {
            Ok(id) => id,
            Err(e) => {
                self.report_peer_error(from, e, data).await;
                return Ok(());
            }
        };

        // Session tracking for IP mismatch (PingMultiIP)
        {
            let mut sessions = self.service.sessions.lock().await;
            if let Some(existing_addr) = sessions.get(&remote_id) {
                if existing_addr.ip() != from.ip() {
                    warn!("[DiscoveryV4] IP mismatch for ID {}: expected {}, got {}", remote_id, existing_addr, from);
                }
            }
            sessions.insert(remote_id, from);
        }

        let packet = match Packet::decode_payload(raw.packet_type, &raw.data) {
            Ok(p) => p,
            Err(e) => {
                self.report_peer_error(from, e, data).await;
                return Ok(());
            }
        };

        match packet {
            Packet::Ping(ping) => {
                self.handle_ping(ping, from, remote_id, raw.hash).await?;
            }
            Packet::Pong(pong) => {
                self.handle_pong(pong, from, remote_id).await?;
            }
            Packet::FindNode(find_node) => {
                self.handle_find_node(find_node, from, remote_id).await?;
            }
            Packet::Neighbors(neighbors) => {
                self.handle_neighbors(neighbors, from, remote_id).await?;
            }
            Packet::ENRRequest(req) => {
                self.handle_enr_request(req, from, remote_id, raw.hash).await?;
            }
            Packet::ENRResponse(_) => {
                debug!("[DiscoveryV4] Received ENRResponse from {}", from);
            }
        }

        Ok(())
    }

    async fn report_peer_error(&self, from: SocketAddr, err: DecodeError, raw_data: &[u8]) {
        let mut errors = self.service.peer_errors.lock().await;
        let (count, last_time) = errors.entry(from).or_insert((0, Instant::now()));
        
        if last_time.elapsed() > Duration::from_secs(10) {
            *count = 0;
            *last_time = Instant::now();
        }
        
        *count += 1;
        
        if *count > 10 {
            debug!("[DiscoveryV4] Error rate limit exceeded for {}: {}", from, err);
        } else {
            match err {
                DecodeError::InvalidPacketType(_) => {
                    debug!("[DiscoveryV4] Invalid packet from {}: {} hex={}", from, err, hex::encode(raw_data));
                }
                _ => {
                    warn!("[DiscoveryV4] Invalid packet from {}: {} hex={}", from, err, hex::encode(raw_data));
                }
            }
        }
    }

    async fn handle_ping(&self, ping: Ping, from: SocketAddr, remote_id: B512, hash: B256) -> Result<()> {
        if ping.expiration <= now_u64() {
            info!("[DiscoveryV4] Expired Ping from {} (exp: {}, now: {})", from, ping.expiration, now_u64());
            return Ok(());
        }

        // Verification of 'from' endpoint (basic amplification protection)
        // Hive expects us to still respond but maybe not update routing if it mismatches?
        // Let's just log and continue for now to avoid timeouts.
        if ping.from.ip != from.ip() {
            info!("[DiscoveryV4] Ping 'from' IP mismatch: {} != {}", ping.from.ip, from.ip());
        }

        info!("[DiscoveryV4] Received Ping from {} (ID: {})", from, remote_id);

        let enr_seq = self.service.local_enr.lock().await.seq;

        // Send Pong
        let pong = Pong {
            to: NodeEndpoint {
                ip: from.ip(),
                udp_port: from.port(),
                tcp_port: ping.from.tcp_port,
            },
            echo: hash,
            expiration: now_u64() + 60,
            enr_seq: Some(enr_seq),
        };
        self.service.send_packet(Packet::Pong(pong), from).await?;

        // If we haven't pinged them yet, we should initiate bonding from our side too
        if !self.service.is_bonded(remote_id).await {
             let _ = self.service.ping_node(from).await;
        }

        Ok(())
    }

    pub async fn handle_pong(&self, pong: Pong, from: SocketAddr, remote_id: B512) -> Result<()> {
        if pong.expiration < now_u64() {
            debug!("[DiscoveryV4] Expired Pong from {}", from);
            return Ok(());
        }
        
        // Verify echo hash and correlation
        {
            let mut pending = self.service.pending_pings.lock().await;
            if let Some(expected_addr) = pending.get(&pong.echo) {
                // Amplification protection: Check if Pong is from the IP we sent Ping to
                if expected_addr.ip() != from.ip() {
                    warn!("[DiscoveryV4] Pong from wrong IP: expected {}, got {}", expected_addr.ip(), from.ip());
                    return Ok(());
                }
                pending.remove(&pong.echo);
            } else {
                debug!("[DiscoveryV4] Unsolicited Pong from {} with echo {}", from, pong.echo);
                return Ok(());
            }
        }

        info!("[DiscoveryV4] Received expected Pong from {} (ID: {})", from, remote_id);

        // Mark bonded by ID
        self.service.bonded_peers.lock().await.insert(remote_id, (from, Instant::now()));
        info!("[DiscoveryV4] Peer {} (ID: {}) is now bonded", from, remote_id);

        // Now safe to add to routing table
        self.service.add_node_to_table(remote_id, NodeEndpoint {
            ip: from.ip(),
            udp_port: from.port(),
            tcp_port: pong.to.tcp_port,
        }).await;

        Ok(())
    }

    async fn handle_find_node(&self, find_node: FindNode, from: SocketAddr, remote_id: B512) -> Result<()> {
        if find_node.expiration <= now_u64() {
            info!("[DiscoveryV4] Expired FindNode from {} (exp: {}, now: {})", from, find_node.expiration, now_u64());
            return Ok(());
        }
        
        // Amplification protection: check if bonded and if IP matches
        {
            let bonded = self.service.bonded_peers.lock().await;
            if let Some((expected_addr, _)) = bonded.get(&remote_id) {
                if expected_addr.ip() != from.ip() {
                     warn!("[DiscoveryV4] FindNode from wrong IP for ID {}: expected {}, got {}", remote_id, expected_addr.ip(), from.ip());
                     return Ok(());
                }
            } else {
                 debug!("[DiscoveryV4] Ignoring FindNode from unbonded peer {}", from);
                 return Ok(());
            }
        }

        info!("[DiscoveryV4] Received FindNode from {} (target: {:?})", from, find_node.target);

        let closest = {
            let rt = self.service.routing_table.lock().await;
            rt.closest_nodes(find_node.target, 16)
        };

        let nodes: Vec<Neighbor> = closest.into_iter().map(|n| Neighbor {
            ip: n.endpoint.ip,
            udp_port: n.endpoint.udp_port,
            tcp_port: n.endpoint.tcp_port,
            id: n.id,
        }).collect();

        let mut neighbors_sent = 0;
        for chunk in nodes.chunks(12) {
            let neighbors = Neighbors {
                nodes: chunk.to_vec(),
                expiration: now_u64() + 60,
            };
            self.service.send_packet(Packet::Neighbors(neighbors), from).await?;
            neighbors_sent += 1;
        }
        info!("[DiscoveryV4] Sent {} Neighbors packets to {}", neighbors_sent, from);
        Ok(())
    }

    pub async fn handle_enr_request(&self, req: ENRRequest, from: SocketAddr, remote_id: B512, hash: B256) -> Result<()> {
        if req.expiration <= now_u64() {
            info!("[DiscoveryV4] Expired ENRRequest from {}", from);
            return Ok(());
        }
        info!("[DiscoveryV4] Received ENRRequest from {}", from);

        if !self.service.is_bonded(remote_id).await {
            debug!("[DiscoveryV4] Ignoring ENRRequest from unbonded peer {}", from);
            return Ok(());
        }

        // Send ENRResponse
        let enr = self.service.get_local_enr_struct().await;
        let resp = ENRResponse {
            reply_token: hash,
            enr,
        };

        self.service.send_packet(Packet::ENRResponse(resp), from).await?;
        Ok(())
    }

    pub async fn handle_neighbors(&self, neighbors: Neighbors, from: SocketAddr, _remote_id: B512) -> Result<()> {
        if neighbors.expiration < now_u64() {
            debug!("[DiscoveryV4] Expired Neighbors from {}", from);
            return Ok(());
        }

        // Amplification protection: Got NEIGHBORS response for FINDNODE from wrong IP
        {
            let mut pending = self.service.pending_find_nodes.lock().await;
            // Clean up old ones first
            pending.retain(|_, (_, time)| time.elapsed() < Duration::from_secs(60));
            
            // We don't necessarily know which target this is a response for if we have multiple,
            // but we should have at least one pending request to this IP.
            if !pending.values().any(|(addr, _)| addr.ip() == from.ip()) {
                 info!("[DiscoveryV4] Unsolicited Neighbors from {} (no pending FindNode)", from);
                 return Ok(());
            }
        }

        info!("[DiscoveryV4] Received {} Neighbors", neighbors.nodes.len());
        for node in neighbors.nodes {
            // Amplification protection: only add if we pinged someone and got these neighbors as a result
            // but devp2p discv4 is a bit more loose. Still, we should only trust bonded nodes for FindNode.
            // The handle_packet ensures we only process neighbors if they come from a known ID,
            // but it doesn't strictly check if we SENT a FindNode.
            
            let addr = SocketAddr::new(node.ip, node.udp_port);
            if !self.service.is_bonded(node.id).await {
                let _ = self.service.ping_node(addr).await;
            }
            self.service.add_node_to_table(node.id, NodeEndpoint {
                ip: node.ip,
                udp_port: node.udp_port,
                tcp_port: node.tcp_port,
            }).await;
        }
        Ok(())
    }
}
