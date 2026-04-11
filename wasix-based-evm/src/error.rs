use thiserror::Error;
use alloy_primitives::B256;
use alloy_eips::BlockId;
use jsonrpsee::types::error::ErrorObjectOwned;

#[derive(Error, Debug)]
pub enum RpcError {
    #[error("Block not found: {0:?}")]
    BlockNotFound(BlockId),
    #[error("Transaction not found: {0:?}")]
    TransactionNotFound(B256),
    #[error("Account not found: {0:?}")]
    AccountNotFound(alloy_primitives::Address),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("State unavailable for block: {0:?}")]
    StateUnavailable(BlockId),
}

impl From<RpcError> for ErrorObjectOwned {
    fn from(err: RpcError) -> Self {
        match err {
            RpcError::BlockNotFound(_) => ErrorObjectOwned::owned(
                -32001, // Custom code for not found
                err.to_string(),
                None::<()>,
            ),
            RpcError::TransactionNotFound(_) => ErrorObjectOwned::owned(
                -32002, 
                err.to_string(),
                None::<()>,
            ),
            RpcError::AccountNotFound(_) => ErrorObjectOwned::owned(
                -32003,
                err.to_string(),
                None::<()>,
            ),
            RpcError::InvalidParams(msg) => ErrorObjectOwned::owned(
                jsonrpsee::types::error::INVALID_PARAMS_CODE,
                msg,
                None::<()>,
            ),
            RpcError::Internal(msg) => ErrorObjectOwned::owned(
                jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                msg,
                None::<()>,
            ),
            RpcError::StateUnavailable(_) => ErrorObjectOwned::owned(
                -32004,
                err.to_string(),
                None::<()>,
            ),
        }
    }
}

pub type RpcResult<T> = Result<T, RpcError>;
