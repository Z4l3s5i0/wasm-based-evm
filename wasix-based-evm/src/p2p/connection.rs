use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfoRlp {
    pub peer_id: String,
    pub addr: String,
}

// These are kept for legacy compatibility if other modules still use them,
// though the RLP-based connection logic is deprecated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Ping,
    Pong,
    Gossip(Vec<u8>),
    PeerList(Vec<PeerInfoRlp>),
}
