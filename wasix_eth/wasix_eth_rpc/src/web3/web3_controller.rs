use async_trait::async_trait;
use wasix_eth_types::web3::Web3ApiServer;
use wasix_eth_types::error::RpcResult;
use wasix_eth_utils::debug;
use crate::web3::web3_service::Web3Service;

pub struct Web3Controller {
    pub service: Web3Service,
}

#[async_trait]
impl Web3ApiServer for Web3Controller {
    async fn client_version(&self) -> RpcResult<String> {
        debug!("[RPC] web3_clientVersion");
        self.service.client_version().await
    }
}
