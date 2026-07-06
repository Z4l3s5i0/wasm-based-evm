use async_trait::async_trait;
use wasix_eth_types::net::{NetApiServer};
use wasix_eth_types::error::RpcResult;
use wasix_eth_types::U256;
use wasix_eth_utils::{debug, metrics::RPC_REQUESTS_TOTAL};
use crate::net::net_service::NetService;

pub struct NetController {
    pub service: NetService,
}

#[async_trait]
impl NetApiServer for NetController {
    async fn peer_count(&self) -> RpcResult<U256> {
        RPC_REQUESTS_TOTAL.inc();
        debug!("[RPC] net_peerCount");
        self.service.peer_count().await
    }
}
