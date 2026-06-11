use alloy_rpc_types::engine::ExecutionPayloadV1;
use alloy_rpc_types::engine::ExecutionPayloadV2 as AlloyExecutionPayloadV2;
use crate::eip4895::Withdrawal;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadV2 {
    #[serde(flatten)]
    pub payload_inner: ExecutionPayloadV1,
    pub withdrawals: Option<Vec<Withdrawal>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadV3 {
    #[serde(flatten)]
    pub payload_inner: ExecutionPayloadV2,
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadV4 {
    #[serde(flatten)]
    pub payload_inner: ExecutionPayloadV3,
}

impl From<ExecutionPayloadV2> for ExecutionPayloadV1 {
    fn from(payload: ExecutionPayloadV2) -> Self {
        payload.payload_inner
    }
}

impl From<ExecutionPayloadV2> for AlloyExecutionPayloadV2 {
    fn from(payload: ExecutionPayloadV2) -> Self {
        Self {
            payload_inner: payload.payload_inner,
            withdrawals: payload.withdrawals.unwrap_or_default(),
        }
    }
}

impl From<AlloyExecutionPayloadV2> for ExecutionPayloadV2 {
    fn from(payload: AlloyExecutionPayloadV2) -> Self {
        Self {
            payload_inner: payload.payload_inner,
            withdrawals: Some(payload.withdrawals),
        }
    }
}
