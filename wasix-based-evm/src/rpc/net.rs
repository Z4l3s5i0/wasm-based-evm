use crate::rpc::{MyTransactionService, NetPeerCountResponse, NetPeersResponse, NetAddPeerRequest, NetAddPeerResponse, NetNodeInfoResponse, Empty};
use crate::rpc::mappers::{status_from, map_peer_info, map_net_add_peer_request, map_node_info_response};
use crate::debug;
use tonic::{Request, Response, Status};

impl MyTransactionService {
    pub async fn net_peer_count_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetPeerCountResponse>, Status> {
        let count = self.provider.peer_count().await.map_err(status_from)?;
        Ok(Response::new(NetPeerCountResponse { count }))
    }

    pub async fn net_peers_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetPeersResponse>, Status> {
        debug!("[RPC] Received net_peers request");
        let peers = self.provider.peers().await.map_err(status_from)?;
        debug!("[RPC] net_peers responded with {} peers", peers.len());
        let proto_peers = peers.into_iter().map(map_peer_info).collect();
        Ok(Response::new(NetPeersResponse { peers: proto_peers }))
    }

    pub async fn net_add_peer_impl(
        &self,
        request: Request<NetAddPeerRequest>,
    ) -> Result<Response<NetAddPeerResponse>, Status> {
        let req = map_net_add_peer_request(request.into_inner());
        self.provider.add_peer(req).await.map_err(status_from)?;
        Ok(Response::new(NetAddPeerResponse { success: true, message: "Peer added".to_string() }))
    }

    pub async fn net_node_info_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetNodeInfoResponse>, Status> {
        let info = self.provider.node_info().await.map_err(status_from)?;
        Ok(Response::new(map_node_info_response(info)))
    }
}