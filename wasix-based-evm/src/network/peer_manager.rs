use std::collections::HashMap;
use tentacle::SessionId;
use crate::network::PeerInfo;
use crate::info;
use crate::debug;

pub struct PeerManager {
    peers: HashMap<SessionId, PeerInfo>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    pub fn add_peer(&mut self, session_id: SessionId, peer_info: PeerInfo) {
        self.peers.insert(session_id, peer_info);
    }

    pub fn remove_peer(&mut self, session_id: &SessionId) -> Option<PeerInfo> {
        self.peers.remove(session_id)
    }

    pub fn get_peer(&self, session_id: &SessionId) -> Option<&PeerInfo> {
        self.peers.get(session_id)
    }

    pub fn get_peer_mut(&mut self, session_id: &SessionId) -> Option<&mut PeerInfo> {
        self.peers.get_mut(session_id)
    }

    pub fn get_all_peers(&self) -> Vec<PeerInfo> {
        self.peers.values().cloned().collect()
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn find_peer_by_addr(&self, addr: &str) -> Option<&PeerInfo> {
        self.peers.values().find(|p| p.addr == addr)
    }

    pub fn report_peer(&mut self, session_id: &SessionId, adjustment: i32) {
        if let Some(peer) = self.peers.get_mut(session_id) {
            peer.reputation += adjustment;
            debug!("[PeerManager] Peer {} reputation adjusted by {}. New reputation: {}", session_id, adjustment, peer.reputation);
        }
    }

    pub fn decay_reputation(&mut self) {
        for peer in self.peers.values_mut() {
            if peer.reputation < 0 {
                peer.reputation += 1;
            } else if peer.reputation > 0 {
                peer.reputation -= 1;
            }
        }
        debug!("[PeerManager] Periodic reputation decay finished");
    }

    // Delegated ServiceEvent handlers
    pub fn on_session_open(&mut self, session_id: SessionId, addr: String) -> PeerManagerEvent {
        // Best-effort duplicate detection by address
        if let Some(existing) = self.find_peer_by_addr(&addr) {
            info!(
                "[PeerManager] Duplicate session detected: addr {} already connected as SessionId({})",
                addr,
                existing.id
            );
        }

        self.add_peer(
            session_id,
            PeerInfo {
                id: session_id.to_string(),
                addr,
                enr: None,
                reputation: 0,
            },
        );

        PeerManagerEvent::Connected(session_id)
    }

    pub fn on_session_close(&mut self, session_id: SessionId) -> PeerManagerEvent {
        let _ = self.remove_peer(&session_id);
        PeerManagerEvent::Disconnected(session_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerManagerEvent {
    Connected(SessionId),
    Disconnected(SessionId),
    None,
}
