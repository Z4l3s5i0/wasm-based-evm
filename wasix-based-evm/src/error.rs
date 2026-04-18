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
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    #[error("Server error: {0}")]
    ServerError(String),
    #[error("Unknown payload: {0}")]
    UnknownPayload(String),
    #[error("Invalid forkchoice state: {0}")]
    InvalidForkchoiceState(String),
    #[error("Invalid payload attributes: {0}")]
    InvalidPayloadAttributes(String),
    #[error("Too large request: {0}")]
    TooLargeRequest(String),
    #[error("Unsupported fork: {0}")]
    UnsupportedFork(String),
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
            RpcError::ParseError(msg) => ErrorObjectOwned::owned(
                jsonrpsee::types::error::PARSE_ERROR_CODE,
                msg,
                None::<()>,
            ),
            RpcError::InvalidRequest(msg) => ErrorObjectOwned::owned(
                jsonrpsee::types::error::INVALID_REQUEST_CODE,
                msg,
                None::<()>,
            ),
            RpcError::MethodNotFound(msg) => ErrorObjectOwned::owned(
                jsonrpsee::types::error::METHOD_NOT_FOUND_CODE,
                msg,
                None::<()>,
            ),
            RpcError::ServerError(msg) => ErrorObjectOwned::owned(
                -32000,
                msg,
                None::<()>,
            ),
            RpcError::UnknownPayload(msg) => ErrorObjectOwned::owned(
                -38001,
                msg,
                None::<()>,
            ),
            RpcError::InvalidForkchoiceState(msg) => ErrorObjectOwned::owned(
                -38002,
                msg,
                None::<()>,
            ),
            RpcError::InvalidPayloadAttributes(msg) => ErrorObjectOwned::owned(
                -38003,
                msg,
                None::<()>,
            ),
            RpcError::TooLargeRequest(msg) => ErrorObjectOwned::owned(
                -38004,
                msg,
                None::<()>,
            ),
            RpcError::UnsupportedFork(msg) => ErrorObjectOwned::owned(
                -38005,
                msg,
                None::<()>,
            ),
        }
    }
}

pub type RpcResult<T> = Result<T, RpcError>;
