use crate::rpc::{MyTransactionService, TransactionRequest, TransactionResponse, ProposeBlockRequest, ProposeBlockResponse, ExecutionPayload, PayloadStatus, ForkchoiceUpdatedRequest, ForkchoiceUpdatedResponse, GetPayloadRequest};
use crate::rpc::mappers::*;
use tonic::{Request, Response, Status};

impl MyTransactionService {
    pub async fn execute_transaction_impl(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let req = map_transaction_request(request.into_inner())?;
        let tx = self.provider.call(req).await.map_err(status_from)?;
        Ok(Response::new(TransactionResponse {
            tx_hash: format!("{:?}", tx.hash),
            success: true,
            message: "Success".to_string(),
            contract_address: String::new(),
            return_data: Vec::new(),
        }))
    }

    pub async fn propose_block_impl(
        &self,
        request: Request<ProposeBlockRequest>,
    ) -> Result<Response<ProposeBlockResponse>, Status> {
        let req = map_propose_block_request(request.into_inner());
        let result = self.provider.propose_block(req).await.map_err(status_from)?;
        
        let tx_results = result.tx_results.into_iter().map(|tx| TransactionResponse {
            success: true,
            message: "Success".to_string(),
            tx_hash: format!("{:?}", tx.hash),
            contract_address: String::new(),
            return_data: Vec::new(),
        }).collect();

        Ok(Response::new(ProposeBlockResponse {
            success: true,
            block_hash: format!("{:?}", result.block_hash),
            message: "Block proposed and executed".to_string(),
            tx_results,
        }))
    }

    pub async fn engine_new_payload_impl(
        &self,
        request: Request<ExecutionPayload>,
    ) -> Result<Response<PayloadStatus>, Status> {
        let payload = map_proto_execution_payload_to_domain(request.into_inner())?;
        let status = self.provider.engine_new_payload(payload).await.map_err(status_from)?;
        Ok(Response::new(map_payload_status(status)))
    }

    pub async fn engine_forkchoice_updated_impl(
        &self,
        request: Request<ForkchoiceUpdatedRequest>,
    ) -> Result<Response<ForkchoiceUpdatedResponse>, Status> {
        let req = map_forkchoice_updated_request(request.into_inner())?;
        let resp = self.provider.engine_forkchoice_updated(req).await.map_err(status_from)?;
        Ok(Response::new(map_forkchoice_updated_response(resp)))
    }

    pub async fn engine_get_payload_impl(
        &self,
        request: Request<GetPayloadRequest>,
    ) -> Result<Response<ExecutionPayload>, Status> {
        let req = map_get_payload_request(request.into_inner())?;
        let payload = self.provider.engine_get_payload(req).await.map_err(status_from)?;
        Ok(Response::new(map_execution_payload_to_proto(payload)))
    }
}


