use serde::{Deserialize, Serialize};
use jsonrpsee::proc_macros::rpc;
use crate::error::RpcResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub enode: String,
    pub enr: Option<String>,
    pub ip: String,
    pub ports: NodePorts,
    pub listen_addr: String,
    pub protocols: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePorts {
    pub discovery: u16,
    pub listener: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub enode: String,
    pub remote_address: String,
    pub local_address: String,
    pub protocols: serde_json::Value,
}

#[rpc(server, client)]
pub trait AdminApi {
    #[method(name = "admin_nodeInfo")]
    async fn node_info(&self) -> RpcResult<NodeInfo>;

    #[method(name = "admin_addPeer")]
    async fn add_peer(&self, enode: String) -> RpcResult<bool>;

    #[method(name = "admin_peers")]
    async fn peers(&self) -> RpcResult<Vec<PeerInfo>>;
}
