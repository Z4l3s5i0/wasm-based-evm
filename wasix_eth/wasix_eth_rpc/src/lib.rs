use wasix_eth_types::eth::EthRpcServer;
use wasix_eth_types::admin::AdminApiServer;
use crate::admin::admin_controller::{AdminController};
pub use crate::admin::admin_service::AdminService;
use crate::debug::debug_controller::{DebugController, DebugRpcServer};
pub use crate::debug::debug_service::DebugService;
use crate::engine::engine_controller::{EngineController, EngineRpcServer};
pub use crate::engine::engine_service::EngineService;
use crate::eth::eth_controller::{EthController};
pub use crate::eth::eth_service::EthService;

mod admin;
mod debug;
mod engine;
mod eth;

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

    pub fn register_admin(&mut self, service: AdminService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(AdminController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn register_engine(&mut self, service: EngineService) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(EngineController { service }.into_rpc()).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn merge_module(&mut self, module: jsonrpsee::RpcModule<()>) -> Result<(), jsonrpsee::types::ErrorObjectOwned> {
        self.module.merge(module).map_err(|e| jsonrpsee::types::ErrorObjectOwned::owned(jsonrpsee::types::error::INTERNAL_ERROR_CODE, e.to_string(), None::<()>))
    }

    pub fn into_module(self) -> jsonrpsee::RpcModule<()> {
        self.module
    }
}




