use crate::rpc::{MyTransactionService, AccountsResponse, BlockNumberResponse, GasPriceResponse, GetBalanceRequest, BalanceResponse, GetBlockByNumberRequest, BlockResponse, GetBlockByHashRequest, GetBlockTransactionCountByHashRequest, TransactionCountResponse, GetBlockTransactionCountByNumberRequest, GetTransactionByHashRequest, TransactionInfoResponse, TransactionReceiptResponse, TransactionRequest, TransactionResponse, GetCodeRequest, CodeResponse, RootsResponse, MempoolResponse, Empty};
use crate::rpc::mappers::{status_from, map_block_response, map_receipt_response, map_tx_info_response, map_transaction_request};
use alloy_primitives::{Address, B256};
use tonic::{Request, Response, Status};

impl MyTransactionService {
    pub async fn eth_accounts_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<AccountsResponse>, Status> {
        let addresses = self.provider.accounts().await.map_err(status_from)?;
        Ok(Response::new(AccountsResponse {
            addresses: addresses.into_iter().map(|a| format!("{:?}", a)).collect(),
        }))
    }

    pub async fn eth_block_number_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<BlockNumberResponse>, Status> {
        let number = self.provider.latest_block_number().await.map_err(status_from)?;
        Ok(Response::new(BlockNumberResponse { number }))
    }

    pub async fn eth_gas_price_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GasPriceResponse>, Status> {
        // Return a fixed gas price for now
        Ok(Response::new(GasPriceResponse { price: 20000000000 }))
    }

    pub async fn eth_get_balance_impl(
        &self,
        request: Request<GetBalanceRequest>,
    ) -> Result<Response<BalanceResponse>, Status> {
        let req = request.into_inner();
        let address: Address = req.address.parse().map_err(|_| Status::invalid_argument("Invalid address"))?;
        let balance = self.provider.balance(address).await.map_err(status_from)?;
        Ok(Response::new(BalanceResponse { balance: balance.to_string() })) // Truncated as per current proto
    }

    pub async fn eth_get_block_by_number_impl(
        &self,
        request: Request<GetBlockByNumberRequest>,
    ) -> Result<Response<BlockResponse>, Status> {
        let req = request.into_inner();
        let block = self.provider.block_by_number(req.number).await.map_err(status_from)?
            .ok_or_else(|| Status::not_found("Block not found"))?;
        Ok(Response::new(map_block_response(&block)))
    }

    pub async fn eth_get_block_by_hash_impl(
        &self,
        request: Request<GetBlockByHashRequest>,
    ) -> Result<Response<BlockResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let block = self.provider.block_by_hash(hash).await.map_err(status_from)?
            .ok_or_else(|| Status::not_found("Block not found"))?;
        Ok(Response::new(map_block_response(&block)))
    }

    pub async fn eth_get_block_transaction_count_by_hash_impl(
        &self,
        request: Request<GetBlockTransactionCountByHashRequest>,
    ) -> Result<Response<TransactionCountResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let count = self.provider.block_transaction_count_by_hash(hash).await.map_err(status_from)?;
        Ok(Response::new(TransactionCountResponse { count }))
    }

    pub async fn eth_get_block_transaction_count_by_number_impl(
        &self,
        request: Request<GetBlockTransactionCountByNumberRequest>,
    ) -> Result<Response<TransactionCountResponse>, Status> {
        let req = request.into_inner();
        let count = self.provider.block_transaction_count_by_number(req.number).await.map_err(status_from)?;
        Ok(Response::new(TransactionCountResponse { count }))
    }

    pub async fn eth_get_transaction_by_hash_impl(
        &self,
        request: Request<GetTransactionByHashRequest>,
    ) -> Result<Response<TransactionInfoResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let tx = self.provider.tx_by_hash(hash).await.map_err(status_from)?
            .ok_or_else(|| Status::not_found("Transaction not found"))?;
        
        let block = self.provider.tx_receipt_by_hash(hash).await.ok().flatten().map(|(_, _, b)| b);
        
        Ok(Response::new(map_tx_info_response(&tx, block.as_ref())))
    }

    pub async fn eth_get_transaction_receipt_impl(
        &self,
        request: Request<GetTransactionByHashRequest>,
    ) -> Result<Response<TransactionReceiptResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        let (tx, receipt, block) = self.provider.tx_receipt_by_hash(hash).await.map_err(status_from)?
            .ok_or_else(|| Status::not_found("Receipt not found"))?;
        
        Ok(Response::new(map_receipt_response(&tx, &receipt, &block)))
    }

    pub async fn eth_send_transaction_impl(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let req = map_transaction_request(request.into_inner())?;
        let tx = self.provider.send_transaction(req).await.map_err(status_from)?;
        Ok(Response::new(TransactionResponse {
            tx_hash: format!("{:?}", tx.hash),
            success: true,
            message: "Transaction sent".to_string(),
            contract_address: String::new(),
            return_data: Vec::new(),
        }))
    }

    pub async fn eth_call_impl(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let req = map_transaction_request(request.into_inner())?;
        let tx = self.provider.call(req).await.map_err(status_from)?;
        Ok(Response::new(TransactionResponse {
            tx_hash: format!("{:?}", tx.hash),
            success: true,
            message: "Call successful".to_string(),
            contract_address: String::new(),
            return_data: Vec::new(),
        }))
    }

    pub async fn eth_get_code_impl(
        &self,
        request: Request<GetCodeRequest>,
    ) -> Result<Response<CodeResponse>, Status> {
        let req = request.into_inner();
        let address: Address = req.address.parse().map_err(|_| Status::invalid_argument("Invalid address"))?;
        let code = self.provider.code_at(address).await.map_err(status_from)?;
        Ok(Response::new(CodeResponse { code }))
    }

    pub async fn eth_get_roots_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<RootsResponse>, Status> {
        let (state_root, transactions_root, receipts_root) = self.provider.roots().await.map_err(status_from)?;
        Ok(Response::new(RootsResponse {
            state_root: format!("{:?}", state_root),
            transactions_root: format!("{:?}", transactions_root),
            receipts_root: receipts_root.map(|r| format!("{:?}", r)).unwrap_or_default(),
            withdrawals_root: String::new(),
        }))
    }

    pub async fn eth_get_mempool_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<MempoolResponse>, Status> {
        let transactions = self.provider.mempool().await.map_err(status_from)?;
        let tx_infos = transactions.into_iter().map(|tx| map_tx_info_response(&tx, None)).collect();
        Ok(Response::new(MempoolResponse { transactions: tx_infos }))
    }
}