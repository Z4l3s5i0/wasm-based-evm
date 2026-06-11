use futures_util::Future;
use jsonrpsee::server::middleware::rpc::{RpcServiceT, MethodResponse};
use jsonrpsee::types::Request;
use tower::Layer;
use hmac::{Hmac, Mac};
use jsonrpsee::core::middleware::{Batch, Notification};
use sha2::Sha256;
use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};
use wasix_eth_types::error::RpcError;
use wasix_eth_utils::error;

#[derive(Clone)]
pub struct HeaderInjectorLayer;

impl<S> Layer<S> for HeaderInjectorLayer {
    type Service = HeaderInjectorService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HeaderInjectorService { inner }
    }
}

#[derive(Clone)]
pub struct HeaderInjectorService<S> {
    inner: S,
}

impl<S, Body> tower::Service<http::Request<Body>> for HeaderInjectorService<S>
where
    S: tower::Service<http::Request<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    Body: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<Body>) -> Self::Future {
        let headers = req.headers().clone();
        req.extensions_mut().insert(headers);
        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(req).await })
    }
}

#[derive(Clone)]
pub struct JwtAuthLayer {
    secret: [u8; 32],
}

impl JwtAuthLayer {
    pub fn new(secret: [u8; 32]) -> Self {
        Self { secret }
    }
}

impl<S> Layer<S> for JwtAuthLayer {
    type Service = JwtAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        JwtAuthService {
            inner,
            secret: self.secret,
        }
    }
}


#[derive(Clone)]
pub struct JwtAuthService<S> {
    inner: S,
    secret: [u8; 32],
}

impl<S> RpcServiceT for JwtAuthService<S>
where
    S: RpcServiceT<MethodResponse = MethodResponse> + Send + Sync + Clone + 'static,
{
    type MethodResponse = MethodResponse;
    type NotificationResponse = S::NotificationResponse;
    type BatchResponse = S::BatchResponse;

    fn call<'a>(&self, req: Request<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
        let secret = self.secret;
        let inner = self.inner.clone();

        async move {
            let id = req.id();
            let method = req.method_name().to_string();

            if !method.starts_with("engine_") {
                return inner.call(req).await;
            }

            let auth_header = req.extensions().get::<http::HeaderMap>()
                .and_then(|h| h.get(http::header::AUTHORIZATION))
                .and_then(|v| v.to_str().ok());

            if let Some(auth_header) = auth_header {
                if auth_header.starts_with("Bearer ") {
                    let token = &auth_header[7..];
                    if validate_jwt(token, &secret) {
                        inner.call(req).await
                    } else {
                        error!("[JWT] Invalid token for method: {}", method);
                        MethodResponse::error(id, RpcError::InvalidJwtToken)
                    }
                } else {
                    error!("[JWT] Invalid Authorization header format for method: {}", method);
                    MethodResponse::error(id, RpcError::InvalidAuthorizationHeader)
                }
            } else {
                error!("[JWT] Missing Authorization header for method: {}", method);
                MethodResponse::error(id, RpcError::MissingAuthorizationHeader)
            }
        }
    }

    fn batch<'a>(&self, requests: Batch<'a>) -> impl Future<Output=Self::BatchResponse> + Send + 'a {
        self.inner.batch(requests)
    }

    fn notification<'a>(&self, n: Notification<'a>) -> impl Future<Output=Self::NotificationResponse> + Send + 'a {
        self.inner.notification(n)
    }
}

pub fn validate_jwt(token: &str, secret: &[u8; 32]) -> bool {
    let mut parts = token.split('.');
    let header_b64 = parts.next();
    let payload_b64 = parts.next();
    let signature_b64 = parts.next();

    if let (Some(header_b64), Some(payload_b64), Some(signature_b64)) = (header_b64, payload_b64, signature_b64) {
        let message = format!("{}.{}", header_b64, payload_b64);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC can take key of any size");
        mac.update(message.as_bytes());

        let payload = match BASE64_URL_SAFE_NO_PAD.decode(payload_b64) {
            Ok(p) => p,
            Err(_) => return false,
        };

        let claims: Claims = match serde_json::from_slice(&payload) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        const MAX_DRIFT: u64 = 60;

        if claims.iat > now + MAX_DRIFT || claims.iat < now - MAX_DRIFT{
            return false;
        }

        if let Ok(signature) = BASE64_URL_SAFE_NO_PAD.decode(signature_b64) {
            return mac.verify_slice(&signature).is_ok();
        }
    }
    false
}

#[derive(serde::Deserialize)]
struct Claims {
    iat: u64,
}