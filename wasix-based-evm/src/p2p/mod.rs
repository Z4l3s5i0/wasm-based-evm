use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use serde::{Deserialize, Serialize};
use alloy_primitives::B256;

pub mod peer_manager;
pub mod rpc_client;
pub mod gossip_handler;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HelloResponse {
    pub peer_id: String,
    pub discovery_addr: String,
    pub p2p_addr: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfoRlp {
    pub peer_id: String,
    pub discovery_addr: String,
    pub p2p_addr: String,
}

#[rpc(server, client)]
pub trait DiscoveryApi {
    #[method(name = "p2p_hello")]
    async fn hello(&self, peer_id: String, discovery_addr: String, p2p_addr: String) -> RpcResult<HelloResponse>;

    #[method(name = "p2p_ping")]
    async fn ping(&self) -> RpcResult<String>;

    #[method(name = "p2p_getPeers")]
    async fn get_peers(&self) -> RpcResult<Vec<PeerInfoRlp>>;
}

#[rpc(server, client)]
pub trait P2pApi {
    #[method(name = "p2p_getBlockByNumber")]
    async fn get_block_by_number(&self, number: u64) -> RpcResult<Option<Vec<u8>>>;

    #[method(name = "p2p_getBlockHash")]
    async fn get_block_hash(&self, number: u64) -> RpcResult<Option<B256>>;

    #[method(name = "p2p_getBlockByHash")]
    async fn get_block_by_hash(&self, hash: B256) -> RpcResult<Option<Vec<u8>>>;

    #[method(name = "p2p_gossip")]
    async fn gossip(&self, topic: String, data: Vec<u8>) -> RpcResult<()>;

    #[method(name = "eth_blockNumber")]
    async fn block_number(&self) -> RpcResult<String>;
}
