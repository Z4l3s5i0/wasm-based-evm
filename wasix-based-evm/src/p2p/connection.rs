use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfoRlp {
    pub peer_id: String,
    pub addr: String,
}
