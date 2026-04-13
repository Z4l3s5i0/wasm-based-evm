use serde::{Deserialize, Serialize};
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;

pub mod identity;
pub mod swarm;
pub mod connection;
pub mod rpc_client;

pub use crate::p2p::connection::PeerInfoRlp;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HelloResponse {
    pub peer_id: String,
}

#[rpc(server, client)]
pub trait P2pApi {
    #[method(name = "p2p_hello")]
    async fn hello(&self, peer_id: String, listen_port: u16) -> RpcResult<HelloResponse>;

    #[method(name = "p2p_ping")]
    async fn ping(&self) -> RpcResult<String>;

    #[method(name = "p2p_getPeers")]
    async fn get_peers(&self) -> RpcResult<Vec<PeerInfoRlp>>;

    #[method(name = "p2p_gossip")]
    async fn gossip(&self, topic: String, data: Vec<u8>) -> RpcResult<()>;
}