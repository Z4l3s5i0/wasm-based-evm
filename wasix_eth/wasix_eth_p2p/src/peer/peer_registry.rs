use anyhow::Result;
use wasix_eth_storage::read_traits::{BlockProvider, PeerDiscoveryProvider, HeaderProvider};
use wasix_eth_types::sync::{P2pSession, PeerProvider};
use std::sync::Arc;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::PeerDiscoveryWriter;
use wasix_eth_types::{async_trait, ChainConfig, PeerEntry, BlockId};
use wasix_eth_utils::identity::Identity;
use wasix_eth_utils::metrics::CONNECTED_PEERS;
use wasix_eth_utils::{debug, info};

use wasix_eth_types::p2p::{DisconnectReason, StatusMessage};
use crate::rlpx::PeerSession;
use std::collections::HashMap;
use alloy_primitives::B256;
use tokio::sync::{mpsc, Mutex};

#[derive(Clone)]
pub struct PeerRegistry {
    pub read_provider: DatabaseReadProvider,
    write_provider: DatabaseWriteProvider,
    local_identity: Identity,
    discovery_port: u16,
    p2p_port: u16,
    pub ext_ip: Option<std::net::IpAddr>,
    active_sessions: Arc<Mutex<HashMap<String, Arc<PeerSession>>>>,
    pub network_id: u64,
    pub genesis_hash: B256,
    pub chain_config: ChainConfig,
    discovery_service_v4: Arc<Mutex<Option<Arc<crate::discovery::v4_service::DiscoveryV4Service>>>>,
    gossip_tx: Arc<Mutex<Option<mpsc::Sender<wasix_eth_types::p2p::GossipMessage>>>>,
    disconnect_tx: mpsc::Sender<String>,
}

#[async_trait]
impl PeerProvider for PeerRegistry {
    async fn get_active_peers(&self) -> Result<Vec<PeerEntry>> {
        let mut peers = self.read_provider.get_active_peers()?;
        let sessions = self.active_sessions.lock().await;

        for (peer_id, session) in sessions.iter() {
            if !peers.iter().any(|p| p.peer_id == *peer_id) {
                peers.push(PeerEntry {
                    peer_id: peer_id.clone(),
                    discovery_addr: session.remote_addr,
                    p2p_addr: session.remote_addr,
                });
            }
        }
        Ok(peers)
    }

    async fn get_session(&self, peer_id: &str) -> Option<Arc<dyn P2pSession>> {
        let sessions = self.active_sessions.lock().await;
        sessions.get(peer_id).cloned().map(|s| s as Arc<dyn P2pSession>)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl PeerRegistry {
    pub fn new(
        read_provider: DatabaseReadProvider,
        write_provider: DatabaseWriteProvider,
        local_identity: Identity,
        discovery_port: u16,
        p2p_port: u16,
        ext_ip: Option<std::net::IpAddr>,
        _bootnodes: Vec<String>,
        network_id: u64,
        genesis_hash: B256,
        chain_config: ChainConfig,
    ) -> Self {
        let (disconnect_tx, mut disconnect_rx) = mpsc::channel(100);
        let registry = Self {
            read_provider,
            write_provider,
            local_identity,
            discovery_port,
            p2p_port,
            ext_ip,
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
            network_id,
            genesis_hash,
            chain_config,
            discovery_service_v4: Arc::new(Mutex::new(None)),
            gossip_tx: Arc::new(Mutex::new(None)),
            disconnect_tx,
        };

        let registry_for_disconnect = registry.clone();
        tokio::spawn(async move {
            while let Some(peer_id) = disconnect_rx.recv().await {
                info!("[P2P] Peer disconnected: {}", peer_id);
                let mut sessions = registry_for_disconnect.active_sessions.lock().await;
                sessions.remove(&peer_id);
                CONNECTED_PEERS.set(sessions.len() as f64);
                let _ = registry_for_disconnect.write_provider.remove_peer(peer_id);
            }
        });

        registry
    }

    pub async fn set_gossip_tx(&self, tx: mpsc::Sender<wasix_eth_types::p2p::GossipMessage>) {
        let mut guard = self.gossip_tx.lock().await;
        *guard = Some(tx);
    }

    pub async fn get_gossip_tx(&self) -> Option<mpsc::Sender<wasix_eth_types::p2p::GossipMessage>> {
        let guard = self.gossip_tx.lock().await;
        guard.clone()
    }

    pub fn disconnect_tx(&self) -> mpsc::Sender<String> {
        self.disconnect_tx.clone()
    }

    pub async fn set_discovery_service_v4(&self, service: Arc<crate::discovery::v4_service::DiscoveryV4Service>) {
        let mut guard = self.discovery_service_v4.lock().await;
        *guard = Some(service);
    }

    pub async fn get_discovery_service_v4(&self) -> Option<Arc<crate::discovery::v4_service::DiscoveryV4Service>> {
        let guard = self.discovery_service_v4.lock().await;
        guard.clone()
    }

    pub fn local_peer_id(&self) -> String {
        self.local_identity.peer_id()
    }

    pub fn p2p_port(&self) -> u16 {
        self.p2p_port
    }

    pub fn discovery_port(&self) -> u16 {
        self.discovery_port
    }

    pub fn local_identity(&self) -> Identity {
        self.local_identity.clone()
    }

    pub fn get_fork_id(&self) -> wasix_eth_types::p2p::ForkId {
        let head_hash = self.read_provider.forkchoice("head").ok().flatten().unwrap_or(self.genesis_hash);
        let head = self.read_provider.header(BlockId::Hash(head_hash.into())).ok().flatten();
        let head_num = head.as_ref().map(|h| h.number).unwrap_or(0);
        let head_time = head.as_ref().map(|h| h.timestamp).unwrap_or(0);

        let genesis = self.read_provider.header(BlockId::Hash(self.genesis_hash.into())).ok().flatten();
        let genesis_time = genesis.as_ref().map(|h| h.timestamp).unwrap_or(0);
        
        wasix_eth_types::p2p::ForkId::new(self.genesis_hash, &self.chain_config, head_num, head_time, genesis_time)
    }

    pub async fn get_session(&self, peer_id: &str) -> Option<Arc<dyn P2pSession>> {
        let sessions = self.active_sessions.lock().await;
        sessions.get(peer_id).cloned().map(|s| s as Arc<dyn P2pSession>)
    }

    pub async fn get_all_sessions(&self) -> Vec<Arc<PeerSession>> {
        let sessions = self.active_sessions.lock().await;
        sessions.values().cloned().collect()
    }

    pub async fn register_session_arc(&self, peer_id: String, session: Arc<PeerSession>) {
        info!("[P2P Registry] Registering session for peer {}", peer_id);
        let mut sessions = self.active_sessions.lock().await;
        
        if let Some(existing) = sessions.get(&peer_id) {
            // Deterministic Tie-Breaking:
            // Use lexicographical comparison of PeerIDs to decide which node should keep the outbound connection.
            // Rule: The node with the HIGHER PeerID is the designated initiator.
            // If we are the node with the higher PeerID, we prefer our OUTBOUND session.
            // If we are the node with the lower PeerID, we prefer our INBOUND session.
            
            let local_id = self.local_peer_id();
            let we_are_higher = local_id > peer_id;
            
            let keep_existing = if we_are_higher {
                existing.is_initiator
            } else {
                !existing.is_initiator
            };

            if keep_existing {
                info!("[P2P Registry] Deterministic tie-break: keeping existing session for peer {}, rejecting new one (local higher: {}, existing initiator: {})", 
                    peer_id, we_are_higher, existing.is_initiator);
                let _ = session.disconnect(DisconnectReason::AlreadyConnected).await;
                return;
            } else {
                info!("[P2P Registry] Deterministic tie-break: replacing existing session for peer {} (local higher: {}, existing initiator: {}, new initiator: {})", 
                    peer_id, we_are_higher, existing.is_initiator, session.is_initiator);
                let _ = existing.disconnect(DisconnectReason::AlreadyConnected).await;
            }
        }
        
        sessions.insert(peer_id, session);
        CONNECTED_PEERS.set(sessions.len() as f64);
    }

    pub async fn cleanup_stale_peers(&self) {
        debug!("[P2P] Starting health check / cleanup cycle...");
        let sessions = self.get_all_sessions().await;
        let now = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(30); // Reduced from 120s

        for session in sessions {
            let last_activity = *session.last_activity.lock().await;
            if now.duration_since(last_activity) > timeout {
                let status = session.status.lock().await;
                let block_hash = status.as_ref().map(|s| match s {
                    StatusMessage::Legacy(l) => l.blockhash,
                    StatusMessage::Eth69(e) => e.blockhash,
                });
                debug!("[P2P] Peer with head {:?} stale for too long ({}s), disconnecting", block_hash, now.duration_since(last_activity).as_secs());
                let _ = session.disconnect(DisconnectReason::PingTimeout).await;
            }
        }
    }
    pub async fn get_active_peers(&self) -> Result<Vec<PeerEntry>> {
        let mut peers = self.read_provider.get_active_peers()?;
        let sessions = self.active_sessions.lock().await;

        for (peer_id, session) in sessions.iter() {
            if !peers.iter().any(|p| p.peer_id == *peer_id) {
                peers.push(PeerEntry {
                    peer_id: peer_id.clone(),
                    discovery_addr: session.remote_addr,
                    p2p_addr: session.remote_addr,
                });
            }
        }
        Ok(peers)
    }

    pub fn register_peer(&self, entry: PeerEntry) -> Result<()> {
        self.write_provider.register_peer(entry)
    }

    pub fn remove_peer(&self, peer_id: String) -> Result<()> {
        self.write_provider.remove_peer(peer_id)
    }
}