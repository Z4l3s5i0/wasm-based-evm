use std::sync::Arc;
use wasix_eth_p2p::PeerManager;
use wasix_eth_types::error::RpcResult;
use wasix_eth_types::U256;

#[derive(Clone)]
pub struct NetService {
    peer_manager: Arc<PeerManager>,
}

impl NetService {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }

    pub async fn peer_count(&self) -> RpcResult<U256> {
        let sessions = self.peer_manager.registry.get_all_sessions().await;
        Ok(U256::from(sessions.len()))
    }
}
