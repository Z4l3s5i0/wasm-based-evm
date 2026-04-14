use std::sync::atomic::{AtomicU64, Ordering};
use std::fmt;
use jsonrpsee::core::DeserializeOwned;
use jsonrpsee::core::traits::ToRpcParams;
use jsonrpsee::core::client::{ClientT, BatchResponse};
use jsonrpsee::core::params::BatchRequestBuilder as CoreBatchRequestBuilder;
use jsonrpsee::core::client::Error;
use jsonrpsee::types::{Request, Response, ResponsePayload, Id};
use std::borrow::Cow;
use std::future::Future;

pub struct RpcClient {
    client: reqwest::Client,
    target_url: String,
    id_counter: AtomicU64,
}

impl RpcClient {
    pub fn new(target_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            target_url,
            id_counter: AtomicU64::new(0),
        }
    }

    fn next_id(&self) -> Id<'static> {
        Id::Number(self.id_counter.fetch_add(1, Ordering::SeqCst))
    }
}

impl ClientT for RpcClient {
    fn notification<Params>(
        &self,
        method: &str,
        params: Params,
    ) -> impl Future<Output = Result<(), Error>> + Send
    where
        Params: ToRpcParams + Send,
    {
        let params_json = params.to_rpc_params()
            .map_err(|e| Error::Custom(e.to_string()));
        
        let client = self.client.clone();
        let target_url = self.target_url.clone();
        let method_str = method.to_string();

        async move {
            let params_json = params_json?;
            let notif = jsonrpsee::types::Notification::new(Cow::Owned(method_str), params_json);
            let body = serde_json::to_string(&notif)
                .map_err(Error::ParseError)?;

            client
                .post(&target_url)
                .body(body)
                .header("Content-Type", "application/json")
                .send()
                .await
                .map_err(|e| Error::Transport(e.into()))?;

            Ok(())
        }
    }

    fn request<R, Params>(
        &self,
        method: &str,
        params: Params,
    ) -> impl Future<Output = Result<R, Error>> + Send
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        let id = self.next_id();
        let params_json = params.to_rpc_params()
            .map_err(|e| Error::Custom(e.to_string()));
            
        let client = self.client.clone();
        let target_url = self.target_url.clone();
        let method_str = method.to_string();

        async move {
            let params_json = params_json?;
            let request = Request::owned(method_str, params_json, id);
            let body = serde_json::to_string(&request)
                .map_err(Error::ParseError)?;

            let response_text = client
                .post(&target_url)
                .body(body)
                .header("Content-Type", "application/json")
                .send()
                .await
                .map_err(|e| Error::Transport(e.into()))?
                .text()
                .await
                .map_err(|e| Error::Transport(e.into()))?;

            // Deserialize to Box<RawValue> first to avoid R: Clone requirement from jsonrpsee_types::Response
            let response: Response<Box<serde_json::value::RawValue>> = serde_json::from_str(&response_text)
                .map_err(Error::ParseError)?;

            match response.payload {
                ResponsePayload::Success(res) => {
                    let val: R = serde_json::from_str(res.get()).map_err(Error::ParseError)?;
                    Ok(val)
                }
                ResponsePayload::Error(err) => Err(Error::Call(err.into_owned())),
            }
        }
    }

    fn batch_request<'a, R>(
        &self,
        _batch: CoreBatchRequestBuilder<'a>,
    ) -> impl Future<Output = Result<BatchResponse<'a, R>, Error>> + Send
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        async move {
            Err(Error::Custom("Batch requests not yet implemented in RpcClient".to_string()))
        }
    }
}
