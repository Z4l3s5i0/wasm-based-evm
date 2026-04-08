use crate::rpc::{MyTransactionService, TransactionRequest, TransactionResponse, ProposeBlockRequest, ProposeBlockResponse, ExecutionPayload, PayloadStatus, ForkchoiceUpdatedRequest, ForkchoiceUpdatedResponse, GetPayloadRequest};
use crate::rpc::mappers::status_from;
use crate::{info, debug};
use tonic::{Request, Response, Status};

impl MyTransactionService {
    pub async fn execute_transaction_impl(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let tx = self.provider.call(request.into_inner()).await.map_err(status_from)?;
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
        let result = self.provider.propose_block(request.into_inner()).await.map_err(status_from)?;
        
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
        let status = self.provider.engine_new_payload(request.into_inner()).await.map_err(status_from)?;
        Ok(Response::new(status))
    }

    pub async fn engine_forkchoice_updated_impl(
        &self,
        request: Request<ForkchoiceUpdatedRequest>,
    ) -> Result<Response<ForkchoiceUpdatedResponse>, Status> {
        let resp = self.provider.engine_forkchoice_updated(request.into_inner()).await.map_err(status_from)?;
        Ok(Response::new(resp))
    }

    pub async fn engine_get_payload_impl(
        &self,
        request: Request<GetPayloadRequest>,
    ) -> Result<Response<ExecutionPayload>, Status> {
        let payload = self.provider.engine_get_payload(request.into_inner()).await.map_err(status_from)?;
        Ok(Response::new(payload))
    }
}


