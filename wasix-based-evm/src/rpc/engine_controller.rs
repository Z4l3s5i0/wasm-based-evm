use crate::error::RpcResult;
use alloy_rpc_types::engine::{
    ExecutionPayloadV1, ExecutionPayloadV2, ForkchoiceState, ForkchoiceUpdated,
    PayloadAttributes, PayloadId, PayloadStatus,
};
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::engine_service::EngineService;

#[rpc(server)]
pub trait EngineRpc {
    #[method(name = "engine_exchangeCapabilities")]
    async fn exchange_capabilities(&self, capabilities: Vec<String>) -> RpcResult<Vec<String>>;

    #[method(name = "engine_forkchoiceUpdatedV1")]
    async fn forkchoice_updated_v1(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated>;

    #[method(name = "engine_forkchoiceUpdatedV2")]
    async fn forkchoice_updated_v2(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated>;

    #[method(name = "engine_getPayloadV1")]
    async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1>;

    #[method(name = "engine_getPayloadV2")]
    async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV2>;

    #[method(name = "engine_newPayloadV1")]
    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> RpcResult<PayloadStatus>;

    #[method(name = "engine_newPayloadV2")]
    async fn new_payload_v2(&self, payload: ExecutionPayloadV2) -> RpcResult<PayloadStatus>;
}

pub struct EngineController {
    pub service: EngineService,
}

#[async_trait]
impl EngineRpcServer for EngineController {
    async fn exchange_capabilities(&self, capabilities: Vec<String>) -> RpcResult<Vec<String>> {
        self.service.exchange_capabilities(capabilities).await
    }

    async fn forkchoice_updated_v1(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        self.service.forkchoice_updated_v1(forkchoice_state, payload_attributes).await
    }

    async fn forkchoice_updated_v2(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        self.service.forkchoice_updated_v2(forkchoice_state, payload_attributes).await
    }

    async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1> {
        self.service.get_payload_v1(payload_id).await
    }

    async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV2> {
        self.service.get_payload_v2(payload_id).await
    }

    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> RpcResult<PayloadStatus> {
        self.service.new_payload_v1(payload).await
    }

    async fn new_payload_v2(&self, payload: ExecutionPayloadV2) -> RpcResult<PayloadStatus> {
        self.service.new_payload_v2(payload).await
    }
}
