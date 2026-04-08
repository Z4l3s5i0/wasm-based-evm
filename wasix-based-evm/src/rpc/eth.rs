use crate::rpc::{MyTransactionService, AccountsResponse, BlockNumberResponse, GasPriceResponse, GetBalanceRequest, BalanceResponse, GetBlockByNumberRequest, BlockResponse, GetBlockByHashRequest, GetBlockTransactionCountByHashRequest, TransactionCountResponse, GetBlockTransactionCountByNumberRequest, GetTransactionByHashRequest, TransactionInfoResponse, TransactionReceiptResponse, TransactionRequest, TransactionResponse, GetCodeRequest, CodeResponse, RootsResponse, MempoolResponse, Empty};
use crate::storage::{InMemoryStorage, types::{Transaction, Block}, Receipt, Withdrawal};
use crate::ev::h160_to_address;
use crate::{info, debug};
use alloy_primitives::{Address, U256, B256, hex};
use tonic::{Request, Response, Status};
use evm::standard::TransactValueCallCreate;

impl MyTransactionService {
    pub async fn eth_accounts_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<AccountsResponse>, Status> {
        self.provider.accounts().await
    }

    pub async fn eth_block_number_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<BlockNumberResponse>, Status> {
        self.provider.latest_block_number().await
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
        self.provider.balance(address).await
    }

    pub async fn eth_get_block_by_number_impl(
        &self,
        request: Request<GetBlockByNumberRequest>,
    ) -> Result<Response<BlockResponse>, Status> {
        let req = request.into_inner();
        self.provider.block_by_number(req.number).await
    }

    pub async fn eth_get_block_by_hash_impl(
        &self,
        request: Request<GetBlockByHashRequest>,
    ) -> Result<Response<BlockResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        self.provider.block_by_hash(hash).await
    }

    pub async fn eth_get_block_transaction_count_by_hash_impl(
        &self,
        request: Request<GetBlockTransactionCountByHashRequest>,
    ) -> Result<Response<TransactionCountResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        self.provider.block_transaction_count_by_hash(hash).await
    }

    pub async fn eth_get_block_transaction_count_by_number_impl(
        &self,
        request: Request<GetBlockTransactionCountByNumberRequest>,
    ) -> Result<Response<TransactionCountResponse>, Status> {
        let req = request.into_inner();
        self.provider.block_transaction_count_by_number(req.number).await
    }

    pub async fn eth_get_transaction_by_hash_impl(
        &self,
        request: Request<GetTransactionByHashRequest>,
    ) -> Result<Response<TransactionInfoResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        self.provider.tx_by_hash(hash).await
    }

    pub async fn eth_get_transaction_receipt_impl(
        &self,
        request: Request<GetTransactionByHashRequest>,
    ) -> Result<Response<TransactionReceiptResponse>, Status> {
        let req = request.into_inner();
        let hash: B256 = req.hash.parse().map_err(|_| Status::invalid_argument("Invalid hash"))?;
        self.provider.tx_receipt_by_hash(hash).await
    }

    pub async fn eth_send_transaction_impl(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        self.provider.send_transaction(req).await
    }

    pub async fn eth_call_impl(
        &self,
        request: Request<TransactionRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        self.provider.call(req).await
    }

    pub async fn eth_get_code_impl(
        &self,
        request: Request<GetCodeRequest>,
    ) -> Result<Response<CodeResponse>, Status> {
        let req = request.into_inner();
        let address: Address = req.address.parse().map_err(|_| Status::invalid_argument("Invalid address"))?;
        self.provider.code_at(address).await
    }

    pub async fn eth_get_roots_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<RootsResponse>, Status> {
        self.provider.roots().await
    }

    pub async fn eth_get_mempool_impl(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<MempoolResponse>, Status> {
        self.provider.mempool().await
    }
}