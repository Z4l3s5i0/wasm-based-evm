mod eth_controller;
mod engine_controller;
mod account_controller;
mod block_controller;
mod transaction_controller;
mod log_controller;

pub mod debug_service;
pub mod eth_service;
pub mod engine_service;
pub mod account_service;
pub mod account_manager;
pub mod block_service;
pub mod transaction_service;
pub mod log_service;
pub mod jwt;

mod account_mapper;
mod block_mapper;
mod transaction_mapper;
mod log_mapper;
mod debug_controller;

use crate::error::RpcResult;
use alloy_primitives::{Address, B256};
use crate::rpc::eth_controller::{EthController, EthRpcServer};
use crate::rpc::eth_service::EthService;
use crate::rpc::engine_controller::{EngineController, EngineRpcServer};
use crate::rpc::engine_service::EngineService;
use crate::rpc::account_controller::{AccountController, AccountRpcServer};
use crate::rpc::account_service::AccountService;
use crate::rpc::block_controller::{BlockController, BlockRpcServer};
use crate::rpc::block_service::BlockService;
use crate::rpc::debug_controller::{DebugController, DebugRpcServer};
use crate::rpc::debug_service::DebugService;
use crate::rpc::transaction_controller::{TransactionController, TransactionRpcServer};
use crate::rpc::transaction_service::TransactionService;
use crate::rpc::log_controller::{LogController, LogRpcServer};
use crate::rpc::log_service::LogService;

pub struct RpcServerFacade {
    module: jsonrpsee::RpcModule<()>,
}

impl RpcServerFacade {
    pub fn new() -> Self {
        Self { module: jsonrpsee::RpcModule::new(()) }
    }

    pub fn empty_module() -> jsonrpsee::RpcModule<()> {
        jsonrpsee::RpcModule::new(())
    }

    pub fn register_accounts(&mut self, service: AccountService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(AccountController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }
    pub fn register_debug(&mut self, service: DebugService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(DebugController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn register_eth(&mut self, service: EthService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(EthController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn register_engine(&mut self, service: EngineService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(EngineController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn register_blocks(&mut self, service: BlockService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(BlockController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn register_transactions(&mut self, service: TransactionService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(TransactionController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn register_logs(&mut self, service: LogService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(LogController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn into_module(self) -> jsonrpsee::RpcModule<()> {
        self.module
    }
}

pub fn parse_address(addr: &str) -> RpcResult<Address> {
    addr.parse().map_err(|e| crate::error::RpcError::InvalidParams(format!("Invalid address: {}", e)))
}

pub fn parse_b256(hash: &str) -> RpcResult<B256> {
    hash.parse().map_err(|e| crate::error::RpcError::InvalidParams(format!("Invalid hash: {}", e)))
}

