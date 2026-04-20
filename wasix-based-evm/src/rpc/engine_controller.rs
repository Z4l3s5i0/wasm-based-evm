use crate::info;
use crate::error::RpcResult;
use alloy_rpc_types::engine::{
    ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3, ExecutionPayloadV4, 
    ForkchoiceState, ForkchoiceUpdated, PayloadAttributes, PayloadId, PayloadStatus, 
    TransitionConfiguration, ExecutionPayloadBodyV1,
};
use alloy_primitives::B256;
use async_trait::async_trait;
use jsonrpsee::proc_macros::rpc;
use crate::rpc::engine_service::EngineService;

#[rpc(server)]
pub trait EngineRpc {
    #[method(name = "engine_exchangeCapabilities")]
    async fn exchange_capabilities(&self, capabilities: Vec<String>) -> RpcResult<Vec<String>>;

    #[method(name = "engine_exchangeTransitionConfigurationV1")]
    async fn exchange_transition_configuration_v1(
        &self,
        config: TransitionConfiguration,
    ) -> RpcResult<TransitionConfiguration>;

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

    #[method(name = "engine_forkchoiceUpdatedV3")]
    async fn forkchoice_updated_v3(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated>;

    #[method(name = "engine_forkchoiceUpdatedV4")]
    async fn forkchoice_updated_v4(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated>;

    #[method(name = "engine_getBlobsV1")]
    async fn get_blobs_v1(&self, indices: Vec<B256>) -> RpcResult<Vec<Option<String>>>;

    #[method(name = "engine_getBlobsV2")]
    async fn get_blobs_v2(&self, indices: Vec<B256>) -> RpcResult<Vec<Option<String>>>;

    #[method(name = "engine_getBlobsV3")]
    async fn get_blobs_v3(&self, indices: Vec<B256>) -> RpcResult<Vec<Option<String>>>;

    #[method(name = "engine_getPayloadBodiesByHashV1")]
    async fn get_payload_bodies_by_hash_v1(
        &self,
        hashes: Vec<B256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>>;

    #[method(name = "engine_getPayloadBodiesByHashV2")]
    async fn get_payload_bodies_by_hash_v2(
        &self,
        hashes: Vec<B256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>>;

    #[method(name = "engine_getPayloadBodiesByRangeV1")]
    async fn get_payload_bodies_by_range_v1(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>>;

    #[method(name = "engine_getPayloadBodiesByRangeV2")]
    async fn get_payload_bodies_by_range_v2(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>>;

    #[method(name = "engine_getPayloadV1")]
    async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1>;

    #[method(name = "engine_getPayloadV2")]
    async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV2>;

    #[method(name = "engine_getPayloadV3")]
    async fn get_payload_v3(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV3>;

    #[method(name = "engine_getPayloadV4")]
    async fn get_payload_v4(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4>;

    #[method(name = "engine_getPayloadV5")]
    async fn get_payload_v5(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4>;

    #[method(name = "engine_getPayloadV6")]
    async fn get_payload_v6(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4>;

    #[method(name = "engine_newPayloadV1")]
    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> RpcResult<PayloadStatus>;

    #[method(name = "engine_newPayloadV2")]
    async fn new_payload_v2(&self, payload: ExecutionPayloadV2) -> RpcResult<PayloadStatus>;

    #[method(name = "engine_newPayloadV3")]
    async fn new_payload_v3(&self, payload: ExecutionPayloadV3) -> RpcResult<PayloadStatus>;

    #[method(name = "engine_newPayloadV4")]
    async fn new_payload_v4(&self, payload: ExecutionPayloadV4) -> RpcResult<PayloadStatus>;

    #[method(name = "engine_newPayloadV5")]
    async fn new_payload_v5(&self, payload: ExecutionPayloadV4) -> RpcResult<PayloadStatus>;
}

pub struct EngineController {
    pub service: EngineService,
}

#[async_trait]
impl EngineRpcServer for EngineController {
    async fn exchange_capabilities(&self, capabilities: Vec<String>) -> RpcResult<Vec<String>> {
        info!("[RPC] engine_exchangeCapabilities: capabilities={:?}", capabilities);
        let result = self.service.exchange_capabilities(capabilities).await?;
        info!("[RPC] engine_exchangeCapabilities result: {:?}", result);
        Ok(result)
    }

    async fn forkchoice_updated_v1(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        info!("[RPC] engine_forkchoiceUpdatedV1: head={:?}, attributes={:?}", forkchoice_state.head_block_hash, payload_attributes);
        let result = self.service.forkchoice_updated_v1(forkchoice_state, payload_attributes).await?;
        info!("[RPC] engine_forkchoiceUpdatedV1 result status={:?}, payload_id={:?}", result.payload_status.status, result.payload_id);
        Ok(result)
    }

    async fn forkchoice_updated_v2(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        info!("[RPC] engine_forkchoiceUpdatedV2: head={:?}, attributes={:?}", forkchoice_state.head_block_hash, payload_attributes);
        let result = self.service.forkchoice_updated_v2(forkchoice_state, payload_attributes).await?;
        info!("[RPC] engine_forkchoiceUpdatedV2 result status={:?}, payload_id={:?}", result.payload_status.status, result.payload_id);
        Ok(result)
    }

    async fn forkchoice_updated_v3(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        info!("[RPC] engine_forkchoiceUpdatedV3: head={:?}, attributes={:?}", forkchoice_state.head_block_hash, payload_attributes);
        let result = self.service.forkchoice_updated_v3(forkchoice_state, payload_attributes).await?;
        info!("[RPC] engine_forkchoiceUpdatedV3 result status={:?}, payload_id={:?}", result.payload_status.status, result.payload_id);
        Ok(result)
    }

    async fn forkchoice_updated_v4(
        &self,
        forkchoice_state: ForkchoiceState,
        payload_attributes: Option<PayloadAttributes>,
    ) -> RpcResult<ForkchoiceUpdated> {
        info!("[RPC] engine_forkchoiceUpdatedV4: head={:?}, attributes={:?}", forkchoice_state.head_block_hash, payload_attributes);
        let result = self.service.forkchoice_updated_v4(forkchoice_state, payload_attributes).await?;
        info!("[RPC] engine_forkchoiceUpdatedV4 result status={:?}, payload_id={:?}", result.payload_status.status, result.payload_id);
        Ok(result)
    }

    async fn exchange_transition_configuration_v1(
        &self,
        config: TransitionConfiguration,
    ) -> RpcResult<TransitionConfiguration> {
        info!("[RPC] engine_exchangeTransitionConfigurationV1: config={:?}", config);
        let result = self.service.exchange_transition_configuration_v1(config).await?;
        info!("[RPC] engine_exchangeTransitionConfigurationV1 result: {:?}", result);
        Ok(result)
    }

    async fn get_blobs_v1(&self, indices: Vec<B256>) -> RpcResult<Vec<Option<String>>> {
        info!("[RPC] engine_getBlobsV1: count={}", indices.len());
        let result = self.service.get_blobs_v1(indices).await?;
        info!("[RPC] engine_getBlobsV1 result count={}", result.len());
        Ok(result)
    }

    async fn get_blobs_v2(&self, indices: Vec<B256>) -> RpcResult<Vec<Option<String>>> {
        info!("[RPC] engine_getBlobsV2: count={}", indices.len());
        let result = self.service.get_blobs_v2(indices).await?;
        info!("[RPC] engine_getBlobsV2 result count={}", result.len());
        Ok(result)
    }

    async fn get_blobs_v3(&self, indices: Vec<B256>) -> RpcResult<Vec<Option<String>>> {
        info!("[RPC] engine_getBlobsV3: count={}", indices.len());
        let result = self.service.get_blobs_v3(indices).await?;
        info!("[RPC] engine_getBlobsV3 result count={}", result.len());
        Ok(result)
    }

    async fn get_payload_bodies_by_hash_v1(
        &self,
        hashes: Vec<B256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        info!("[RPC] engine_getPayloadBodiesByHashV1: count={}", hashes.len());
        let result = self.service.get_payload_bodies_by_hash_v1(hashes).await?;
        info!("[RPC] engine_getPayloadBodiesByHashV1 result count={}", result.len());
        Ok(result)
    }

    async fn get_payload_bodies_by_hash_v2(
        &self,
        hashes: Vec<B256>,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        info!("[RPC] engine_getPayloadBodiesByHashV2: count={}", hashes.len());
        let result = self.service.get_payload_bodies_by_hash_v2(hashes).await?;
        info!("[RPC] engine_getPayloadBodiesByHashV2 result count={}", result.len());
        Ok(result)
    }

    async fn get_payload_bodies_by_range_v1(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        info!("[RPC] engine_getPayloadBodiesByRangeV1: start={}, count={}", start, count);
        let result = self.service.get_payload_bodies_by_range_v1(start, count).await?;
        info!("[RPC] engine_getPayloadBodiesByRangeV1 result count={}", result.len());
        Ok(result)
    }

    async fn get_payload_bodies_by_range_v2(
        &self,
        start: u64,
        count: u64,
    ) -> RpcResult<Vec<Option<ExecutionPayloadBodyV1>>> {
        info!("[RPC] engine_getPayloadBodiesByRangeV2: start={}, count={}", start, count);
        let result = self.service.get_payload_bodies_by_range_v2(start, count).await?;
        info!("[RPC] engine_getPayloadBodiesByRangeV2 result count={}", result.len());
        Ok(result)
    }

    async fn get_payload_v1(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV1> {
        info!("[RPC] engine_getPayloadV1: payload_id={:?}", payload_id);
        let result = self.service.get_payload_v1(payload_id).await?;
        info!("[RPC] engine_getPayloadV1 result: hash={:?}", result.block_hash);
        Ok(result)
    }

    async fn get_payload_v2(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV2> {
        info!("[RPC] engine_getPayloadV2: payload_id={:?}", payload_id);
        let result = self.service.get_payload_v2(payload_id).await?;
        info!("[RPC] engine_getPayloadV2 result: hash={:?}", result.payload_inner.block_hash);
        Ok(result)
    }

    async fn get_payload_v3(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV3> {
        info!("[RPC] engine_getPayloadV3: payload_id={:?}", payload_id);
        let result = self.service.get_payload_v3(payload_id).await?;
        info!("[RPC] engine_getPayloadV3 result: hash={:?}", result.payload_inner.payload_inner.block_hash);
        Ok(result)
    }

    async fn get_payload_v4(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4> {
        info!("[RPC] engine_getPayloadV4: payload_id={:?}", payload_id);
        let result = self.service.get_payload_v4(payload_id).await?;
        info!("[RPC] engine_getPayloadV4 result: hash={:?}", result.payload_inner.payload_inner.payload_inner.block_hash);
        Ok(result)
    }

    async fn get_payload_v5(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4> {
        info!("[RPC] engine_getPayloadV5: payload_id={:?}", payload_id);
        let result = self.service.get_payload_v5(payload_id).await?;
        info!("[RPC] engine_getPayloadV5 result: hash={:?}", result.payload_inner.payload_inner.payload_inner.block_hash);
        Ok(result)
    }

    async fn get_payload_v6(&self, payload_id: PayloadId) -> RpcResult<ExecutionPayloadV4> {
        info!("[RPC] engine_getPayloadV6: payload_id={:?}", payload_id);
        let result = self.service.get_payload_v6(payload_id).await?;
        info!("[RPC] engine_getPayloadV6 result: hash={:?}", result.payload_inner.payload_inner.payload_inner.block_hash);
        Ok(result)
    }

    async fn new_payload_v1(&self, payload: ExecutionPayloadV1) -> RpcResult<PayloadStatus> {
        info!("[RPC] engine_newPayloadV1: block_number={}, block_hash={:?}", payload.block_number, payload.block_hash);
        let result = self.service.new_payload_v1(payload).await?;
        info!("[RPC] engine_newPayloadV1 result status={:?}, latest_valid_hash={:?}", result.status, result.latest_valid_hash);
        Ok(result)
    }

    async fn new_payload_v2(&self, payload: ExecutionPayloadV2) -> RpcResult<PayloadStatus> {
        info!("[RPC] engine_newPayloadV2: block_number={}, block_hash={:?}", payload.payload_inner.block_number, payload.payload_inner.block_hash);
        let result = self.service.new_payload_v2(payload).await?;
        info!("[RPC] engine_newPayloadV2 result status={:?}, latest_valid_hash={:?}", result.status, result.latest_valid_hash);
        Ok(result)
    }

    async fn new_payload_v3(&self, payload: ExecutionPayloadV3) -> RpcResult<PayloadStatus> {
        info!("[RPC] engine_newPayloadV3: block_number={}, block_hash={:?}", payload.payload_inner.payload_inner.block_number, payload.payload_inner.payload_inner.block_hash);
        let result = self.service.new_payload_v3(payload).await?;
        info!("[RPC] engine_newPayloadV3 result status={:?}, latest_valid_hash={:?}", result.status, result.latest_valid_hash);
        Ok(result)
    }

    async fn new_payload_v4(&self, payload: ExecutionPayloadV4) -> RpcResult<PayloadStatus> {
        info!("[RPC] engine_newPayloadV4: block_number={}, block_hash={:?}", payload.payload_inner.payload_inner.payload_inner.block_number, payload.payload_inner.payload_inner.payload_inner.block_hash);
        let result = self.service.new_payload_v4(payload).await?;
        info!("[RPC] engine_newPayloadV4 result status={:?}, latest_valid_hash={:?}", result.status, result.latest_valid_hash);
        Ok(result)
    }

    async fn new_payload_v5(&self, payload: ExecutionPayloadV4) -> RpcResult<PayloadStatus> {
        info!("[RPC] engine_newPayloadV5: block_number={}, block_hash={:?}", payload.payload_inner.payload_inner.payload_inner.block_number, payload.payload_inner.payload_inner.payload_inner.block_hash);
        let result = self.service.new_payload_v5(payload).await?;
        info!("[RPC] engine_newPayloadV5 result status={:?}, latest_valid_hash={:?}", result.status, result.latest_valid_hash);
        Ok(result)
    }

}
