use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};

pub mod identity;
pub mod peer_manager;
pub mod rpc_client;
pub mod gossip_handler;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HelloResponse {
    pub peer_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfoRlp {
    pub peer_id: String,
    pub addr: String,
}

#[rpc(server, client)]
pub trait P2pApi {
    #[method(name = "p2p_hello")]
    async fn hello(&self, peer_id: String, public_addr: String) -> RpcResult<HelloResponse>;

    #[method(name = "p2p_ping")]
    async fn ping(&self) -> RpcResult<String>;

    #[method(name = "p2p_getPeers")]
    async fn get_peers(&self) -> RpcResult<Vec<PeerInfoRlp>>;

    #[method(name = "p2p_gossip")]
    async fn gossip(&self, topic: String, data: Vec<u8>) -> RpcResult<()>;
}