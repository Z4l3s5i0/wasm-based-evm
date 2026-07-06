use async_trait::async_trait;
use wasix_eth_types::admin::{AdminApiServer, NodeInfo, PeerInfo};
use wasix_eth_types::error::RpcResult;
use wasix_eth_utils::{debug, metrics::RPC_REQUESTS_TOTAL};
use crate::admin::admin_service::AdminService;

pub struct AdminController {
    pub service: AdminService,
}

#[async_trait]
impl AdminApiServer for AdminController {
    async fn node_info(&self) -> RpcResult<NodeInfo> {
        RPC_REQUESTS_TOTAL.inc();
        debug!("[RPC] admin_nodeInfo");
        let info = self.service.node_info().await.map_err(|e| wasix_eth_types::error::RpcError::Internal(e.to_string()))?;
        Ok(info)
    }

    async fn add_peer(&self, enode: String) -> RpcResult<bool> {
        RPC_REQUESTS_TOTAL.inc();
        debug!("[RPC] admin_addPeer: {}", enode);
        let res = self.service.add_peer(enode).await.map_err(|e| wasix_eth_types::error::RpcError::Internal(e.to_string()))?;
        Ok(res)
    }

    async fn peers(&self) -> RpcResult<Vec<PeerInfo>> {
        RPC_REQUESTS_TOTAL.inc();
        debug!("[RPC] admin_peers");
        let peers = self.service.peers().await.map_err(|e| wasix_eth_types::error::RpcError::Internal(e.to_string()))?;
        Ok(peers)
    }
}
