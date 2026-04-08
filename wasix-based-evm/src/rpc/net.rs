use crate::rpc::{MyTransactionService, NetPeerCountResponse, NetPeersResponse, NetAddPeerRequest, NetAddPeerResponse, NetNodeInfoResponse, Empty};
use crate::{info, debug};
use tonic::{Request, Response, Status};

impl MyTransactionService {
    pub async fn net_peer_count_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetPeerCountResponse>, Status> {
        self.provider.net_peer_count().await
    }

    pub async fn net_peers_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetPeersResponse>, Status> {
        debug!("[RPC] Received net_peers request");
        let res = self.provider.net_peers().await;
        if let Ok(ref r) = res { debug!("[RPC] net_peers responded with {} peers", r.get_ref().peers.len()); }
        res
    }

    pub async fn net_add_peer_impl(
        &self,
        request: Request<NetAddPeerRequest>,
    ) -> Result<Response<NetAddPeerResponse>, Status> {
        let req = request.into_inner();
        self.provider.net_add_peer(req).await
    }

    pub async fn net_node_info_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetNodeInfoResponse>, Status> {
        self.provider.net_node_info().await
    }
}