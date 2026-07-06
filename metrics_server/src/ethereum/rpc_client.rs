use serde_json::{json, Value};
use anyhow::{Result, anyhow};
use std::time::Duration;
use tracing::{debug, error};

#[derive(Clone)]
pub struct EthereumRpcClient {
    http: reqwest::Client,
    rpc_url: String,
}

impl EthereumRpcClient {
    pub fn new(rpc_url: String, timeout: Duration) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()?;
        Ok(Self { http, rpc_url })
    }

    pub async fn call<T>(&self, method: &str, params: Value) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        debug!(method = method, url = %self.rpc_url, "Sending RPC request");

        let response = self.http.post(&self.rpc_url)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            error!(method = method, status = %response.status(), "RPC request failed with HTTP error");
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let res_json: Value = response.json().await?;
        
        if let Some(error) = res_json.get("error") {
            error!(method = method, error = %error, "RPC returned error");
            return Err(anyhow!("RPC error: {}", error));
        }

        let result = res_json.get("result")
            .ok_or_else(|| anyhow!("missing 'result' in RPC response"))?;

        debug!(method = method, "RPC request successful");
        Ok(serde_json::from_value(result.clone())?)
    }

    pub async fn chain_id(&self) -> Result<Value> {
        self.call("eth_chainId", json!([])).await
    }

    pub async fn block_number(&self) -> Result<Value> {
        self.call("eth_blockNumber", json!([])).await
    }

    pub async fn syncing(&self) -> Result<Value> {
        self.call("eth_syncing", json!([])).await
    }

    pub async fn gas_price(&self) -> Result<Value> {
        self.call("eth_gasPrice", json!([])).await
    }

    pub async fn peer_count(&self) -> Result<Value> {
        self.call("net_peerCount", json!([])).await
    }

    pub async fn client_version(&self) -> Result<String> {
        self.call("web3_clientVersion", json!([])).await
    }
}
