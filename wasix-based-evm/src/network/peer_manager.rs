use std::collections::HashMap;
use tentacle::SessionId;
use crate::network::PeerInfo;
use crate::info;
use crate::debug;

#[derive(Clone)]
struct PendingPeerMeta {
    addr_prefix: String,
    enr: String,
}

pub struct PeerManager {
    peers: HashMap<SessionId, PeerInfo>,
    // Pending metadata captured before a session opens (e.g., from ENR dialing)
    pending: Vec<PendingPeerMeta>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
            pending: Vec::new(),
        }
    }

    pub fn add_peer(&mut self, session_id: SessionId, peer_info: PeerInfo) {
        self.peers.insert(session_id, peer_info);
    }

    // Register ENR and preferred addr prefix for an upcoming connection attempt
    pub fn register_pending_enr(&mut self, addr_prefix: String, enr: String) {
        debug!("[PeerManager] Registered pending ENR for addr_prefix {}", addr_prefix);
        self.pending.push(PendingPeerMeta { addr_prefix, enr });
    }

    // Try to match a session address to a previously registered pending entry (by prefix)
    pub fn take_pending_for_session_addr(&mut self, session_addr: &str) -> Option<PendingPeerMeta> {
        if let Some(idx) = self
            .pending
            .iter()
            .position(|p| session_addr.starts_with(&p.addr_prefix))
        {
            Some(self.pending.remove(idx))
        } else {
            None
        }
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

        // Start with raw session-provided addr
        let mut final_addr = addr.clone();
        let mut final_enr: Option<String> = None;

        // If we have a pending record for this connection, use its ENR and
        // prefer its addr when the session-provided addr looks bogus (0.0.0.0:0)
        if let Some(pending) = self.take_pending_for_session_addr(&addr) {
            final_enr = Some(pending.enr.clone());
            // Heuristic: treat addresses containing "/ip4/0.0.0.0" or "/tcp/0" as invalid and
            // prefer the dialed address prefix which should include a concrete ip/port.
            if addr.contains("/ip4/0.0.0.0") || addr.contains("/tcp/0") {
                final_addr = pending.addr_prefix;
            }
        }

        self.add_peer(
            session_id,
            PeerInfo {
                id: session_id.to_string(),
                addr: final_addr,
                enr: final_enr,
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
