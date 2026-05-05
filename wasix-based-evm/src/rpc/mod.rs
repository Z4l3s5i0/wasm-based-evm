mod eth_controller;
mod engine_controller;
pub mod debug_service;
pub mod eth_service;
pub mod engine_service;
pub mod account_manager;

mod block_mapper;
pub mod engine_mapper;
mod transaction_mapper;
mod debug_controller;

use alloy_primitives::Address;
use crate::misc::error::RpcResult;
use crate::rpc::eth_controller::{EthController, EthRpcServer};
use crate::rpc::eth_service::EthService;
use crate::rpc::engine_controller::{EngineController, EngineRpcServer};
use crate::rpc::engine_service::EngineService;
use crate::rpc::debug_controller::{DebugController, DebugRpcServer};
use crate::rpc::debug_service::DebugService;

pub struct RpcServerFacade {
    module: jsonrpsee::RpcModule<()>,
}

impl RpcServerFacade {
    pub fn new() -> Self {
        Self { module: jsonrpsee::RpcModule::new(()) }
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

    pub fn into_module(self) -> jsonrpsee::RpcModule<()> {
        self.module
    }
}

pub fn parse_address(addr: &str) -> RpcResult<Address> {
    addr.parse().map_err(|e| crate::misc::error::RpcError::InvalidParams(format!("Invalid address: {}", e)))
}

