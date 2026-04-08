use crate::rpc::{MyTransactionService, NetPeerCountResponse, NetPeersResponse, NetAddPeerRequest, NetAddPeerResponse, NetNodeInfoResponse, Empty, PeerInfo as ProtoPeerInfo};
use crate::{info, debug};
use tonic::{Request, Response, Status};
use tokio::sync::{oneshot};

impl MyTransactionService {
    pub async fn net_peer_count_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetPeerCountResponse>, Status> {
        if let Some(handle) = &self.network_handle {
            let (tx, rx) = oneshot::channel();
            handle.network_send.send(crate::network::NetworkMessage::GetPeerCount(tx)).await
                .map_err(|_| Status::internal("Failed to send to network service"))?;

            let count = tokio::time::timeout(std::time::Duration::from_secs(5), rx).await
                .map_err(|_| Status::deadline_exceeded("Network service timed out"))?
                .map_err(|_| Status::internal("Failed to receive from network service"))?;

            Ok(Response::new(NetPeerCountResponse { count: count as u64 }))
        } else {
            Err(Status::unavailable("Network not started"))
        }
    }

    pub async fn net_peers_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetPeersResponse>, Status> {
        debug!("[RPC] Received net_peers request");
        if let Some(handle) = &self.network_handle {
            let (tx, rx) = oneshot::channel();
            debug!("[RPC] Sending GetPeers to network service");
            handle.network_send.send(crate::network::NetworkMessage::GetPeers(tx)).await
                .map_err(|_| Status::internal("Failed to send to network service"))?;

            debug!("[RPC] Awaiting response for GetPeers from network service");
            let peers = tokio::time::timeout(std::time::Duration::from_secs(5), rx).await
                .map_err(|_| Status::deadline_exceeded("Network service timed out"))?
                .map_err(|_| Status::internal("Failed to receive from network service"))?;

            debug!("[RPC] Received {} peers from network service", peers.len());
            let proto_peers = peers.into_iter().map(|p| ProtoPeerInfo {
                id: p.id,
                addr: p.addr,
                enr: p.enr.unwrap_or_default(),
            }).collect();
            Ok(Response::new(NetPeersResponse { peers: proto_peers }))
        } else {
            Err(Status::unavailable("Network not started"))
        }
    }

    pub async fn net_add_peer_impl(
        &self,
        request: Request<NetAddPeerRequest>,
    ) -> Result<Response<NetAddPeerResponse>, Status> {
        let req = request.into_inner();
        if let Some(handle) = &self.network_handle {
            let (tx, rx) = oneshot::channel();
            handle.network_send.send(crate::network::NetworkMessage::AddPeer(req.addr, tx)).await
                .map_err(|_| Status::internal("Failed to send to network service"))?;
            match rx.await.map_err(|_| Status::internal("Failed to receive from network service"))? {
                Ok(_) => Ok(Response::new(NetAddPeerResponse { success: true, message: "Peer added".to_string() })),
                Err(e) => Ok(Response::new(NetAddPeerResponse { success: false, message: e })),
            }
        } else {
            Err(Status::unavailable("Network not started"))
        }
    }

    pub async fn net_node_info_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetNodeInfoResponse>, Status> {
        if let Some(handle) = &self.network_handle {
            let (tx, rx) = oneshot::channel();
            handle.network_send.send(crate::network::NetworkMessage::GetNodeInfo(tx)).await
                .map_err(|_| Status::internal("Failed to send to network service"))?;
            let info = rx.await.map_err(|_| Status::internal("Failed to receive from network service"))?;
            Ok(Response::new(NetNodeInfoResponse {
                enr: info.enr,
                node_id: info.node_id,
                listen_addresses: info.listen_addresses,
            }))
        } else {
            Err(Status::unavailable("Network not started"))
        }
    }
}