use crate::rpc::{MyTransactionService, TransactionRequest, TransactionResponse, ProposeBlockRequest, ProposeBlockResponse, ExecutionPayload, PayloadStatus, ForkchoiceUpdatedRequest, ForkchoiceUpdatedResponse, GetPayloadRequest, PendingPayload};
use crate::storage::types::{Block, Transaction};
use crate::ev::h160_to_address;
use crate::{info, debug};
use alloy_primitives::{Address, U256, B256, hex};
use tonic::{Request, Response, Status};
use alloy_rlp::Decodable;
use evm::standard::TransactValueCallCreate;

impl MyTransactionService {
    pub async fn execute_transaction_impl(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        // Reuse eth_call path via provider::call for execution-only semantics
        // Or consider adding a dedicated provider method if semantics differ.
        self.provider.call(request.into_inner()).await
    }



    pub async fn propose_block_impl(
        &self,
        request: Request<ProposeBlockRequest>,
    ) -> Result<Response<ProposeBlockResponse>, Status> {
        self.provider.propose_block(request.into_inner()).await
    }

    pub async fn engine_new_payload_impl(
        &self,
        request: Request<ExecutionPayload>,
    ) -> Result<Response<PayloadStatus>, Status> {
        self.provider.engine_new_payload(request.into_inner()).await
    }

    pub async fn engine_forkchoice_updated_impl(
        &self,
        request: Request<ForkchoiceUpdatedRequest>,
    ) -> Result<Response<ForkchoiceUpdatedResponse>, Status> {
        self.provider.engine_forkchoice_updated(request.into_inner()).await
    }

    pub async fn engine_get_payload_impl(
        &self,
        request: Request<GetPayloadRequest>,
    ) -> Result<Response<ExecutionPayload>, Status> {
        self.provider.engine_get_payload(request.into_inner()).await
    }
}


