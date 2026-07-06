use wasix_eth_types::error::RpcResult;

#[derive(Clone, Default)]
pub struct Web3Service;

impl Web3Service {
    pub fn new() -> Self {
        Self
    }

    pub async fn client_version(&self) -> RpcResult<String> {
        let version = "0.1.0";
        let platform = if cfg!(target_family = "wasm") {
            "wasix"
        } else {
            "rust"
        };
        
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;
        
        Ok(format!("wasix-eth/v{}/{}-{}/{}", version, platform, os, arch))
    }
}
