use alloy_eips::BlockId;
use alloy_primitives::{Address, B256};
use std::str::FromStr;
use thiserror::Error;
use jsonrpsee::types::error::ErrorObjectOwned;

#[derive(Error, Debug, Clone)]
pub enum RpcError {
    #[error("Block not found: {0:?}")]
    BlockNotFound(BlockId),
    #[error("Transaction not found: {0:?}")]
    TransactionNotFound(B256),
    #[error("Account not found: {0:?}")]
    AccountNotFound(Address),
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
    #[error("Invalid params: {0}")]
    InvalidParamsCode(String),
    #[error("Invalid payload attributes: {0}")]
    InvalidPayloadAttributes(String),
    #[error("Too large request: {0}")]
    TooLargeRequest(String),
    #[error("Unsupported fork: {0}")]
    UnsupportedFork(String),
    #[error("Missing Authorization header")]
    MissingAuthorizationHeader,
    #[error("Invalid Authorization header format")]
    InvalidAuthorizationHeader,
    #[error("Invalid JWT token")]
    InvalidJwtToken,
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
            RpcError::InvalidParamsCode(msg) => ErrorObjectOwned::owned(
                -32602,
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
            RpcError::MissingAuthorizationHeader => ErrorObjectOwned::owned(
                -32000,
                err.to_string(),
                None::<()>,
            ),
            RpcError::InvalidAuthorizationHeader => ErrorObjectOwned::owned(
                -32000,
                err.to_string(),
                None::<()>,
            ),
            RpcError::InvalidJwtToken => ErrorObjectOwned::owned(
                -32000,
                err.to_string(),
                None::<()>,
            ),
        }
    }
}

pub type RpcResult<T> = Result<T, RpcError>;


pub fn parse_strict_hex<T: std::str::FromStr>(v: serde_json::Value) -> RpcResult<T> {
    let s = v.as_str().ok_or_else(|| RpcError::InvalidParamsCode("Invalid params".to_string()))?;
    if !s.starts_with("0x") {
        return Err(RpcError::InvalidParamsCode("hex string without 0x prefix".to_string()));
    }
    T::from_str(s).map_err(|_| RpcError::InvalidParamsCode("Invalid params".to_string()))
}

pub fn parse_loose_hash(v: serde_json::Value) -> RpcResult<B256> {
    let s = v.as_str().ok_or_else(|| RpcError::InvalidParamsCode("Invalid params".to_string()))?;
    if !s.starts_with("0x") {
        return Err(RpcError::InvalidParamsCode("hex string without 0x prefix".to_string()));
    }
    let hex_part = &s[2..];
    if hex_part.len() > 64 {
        return Err(RpcError::InvalidParamsCode("hex string too long for B256".to_string()));
    }
    
    if hex_part.len() == 64 {
        return B256::from_str(s).map_err(|_| RpcError::InvalidParamsCode("Invalid params".to_string()));
    }

    let mut full_hex = String::with_capacity(66);
    full_hex.push_str("0x");
    for _ in 0..(64 - hex_part.len()) {
        full_hex.push('0');
    }
    full_hex.push_str(hex_part);
    
    B256::from_str(&full_hex).map_err(|_| RpcError::InvalidParamsCode("Invalid params".to_string()))
}

pub fn parse_loose_address(v: serde_json::Value) -> RpcResult<Address> {
    let s = v.as_str().ok_or_else(|| RpcError::InvalidParamsCode("Invalid params".to_string()))?;
    if !s.starts_with("0x") {
        return Err(RpcError::InvalidParamsCode("hex string without 0x prefix".to_string()));
    }
    let hex_part = &s[2..];
    if hex_part.len() > 40 {
        return Err(RpcError::InvalidParamsCode("hex string too long for Address".to_string()));
    }

    if hex_part.len() == 40 {
        return Address::from_str(s).map_err(|_| RpcError::InvalidParamsCode("Invalid params".to_string()));
    }

    let mut full_hex = String::with_capacity(42);
    full_hex.push_str("0x");
    for _ in 0..(40 - hex_part.len()) {
        full_hex.push('0');
    }
    full_hex.push_str(hex_part);

    Address::from_str(&full_hex).map_err(|_| RpcError::InvalidParamsCode("Invalid params".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;
    use serde_json::json;
    use std::str::FromStr;

    #[test]
    fn test_parse_loose_hash() {
        // Full length
        let full = "0x0000000000000000000000000000000000000000000000000000000000000079";
        let v = json!(full);
        let res = parse_loose_hash(v).unwrap();
        assert_eq!(res, B256::from_str(full).unwrap());

        // Compact length
        let compact = "0x79";
        let v = json!(compact);
        let res = parse_loose_hash(v).unwrap();
        assert_eq!(res, B256::from_str(full).unwrap());

        // Another compact
        let compact2 = "0x123456";
        let v = json!(compact2);
        let res = parse_loose_hash(v).unwrap();
        assert_eq!(res, B256::from_str("0x0000000000000000000000000000000000000000000000000000000000123456").unwrap());
        
        // Invalid
        let invalid = json!("0xG");
        assert!(parse_loose_hash(invalid).is_err());
        
        let too_long = json!("0x0000000000000000000000000000000000000000000000000000000000000000000000000000000079");
        assert!(parse_loose_hash(too_long).is_err());
    }

    #[test]
    fn test_parse_loose_address() {
        // Full length
        let full = "0x0000000000000000000000000000000000000079";
        let v = json!(full);
        let res = parse_loose_address(v).unwrap();
        assert_eq!(res, Address::from_str(full).unwrap());

        // Compact length
        let compact = "0x79";
        let v = json!(compact);
        let res = parse_loose_address(v).unwrap();
        assert_eq!(res, Address::from_str(full).unwrap());

        // Invalid
        let invalid = json!("0xG");
        assert!(parse_loose_address(invalid).is_err());
    }
}

