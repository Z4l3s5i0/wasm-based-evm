use crate::error::RpcError;
use futures_util::Future;
use jsonrpsee::server::middleware::rpc::{RpcServiceT, MethodResponse};
use jsonrpsee::types::Request;
use tower::Layer;
use hmac::{Hmac, Mac};
use jsonrpsee::core::middleware::{Batch, Notification};
use sha2::Sha256;
use base64::Engine;
use base64::prelude::BASE64_URL_SAFE_NO_PAD;

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
            let auth_header = req.extensions().get::<http::HeaderMap>()
                .and_then(|h| h.get(http::header::AUTHORIZATION))
                .and_then(|v| v.to_str().ok());

            if let Some(auth_header) = auth_header {
                if auth_header.starts_with("Bearer ") {
                    let token = &auth_header[7..];
                    if validate_jwt(token, &secret) {
                        return inner.call(req).await;
                    } else {
                        crate::error!("[JWT] Invalid token for method: {}", method);
                        return MethodResponse::error(id, RpcError::InvalidJwtToken);
                    }
                } else {
                    crate::error!("[JWT] Invalid Authorization header format for method: {}", method);
                    return MethodResponse::error(id, RpcError::InvalidAuthorizationHeader);
                }
            } else {
                crate::error!("[JWT] Missing Authorization header for method: {}", method);
                return MethodResponse::error(id, RpcError::MissingAuthorizationHeader);
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

fn validate_jwt(token: &str, secret: &[u8; 32]) -> bool {
    let mut parts = token.split('.');
    let header_b64 = parts.next();
    let payload_b64 = parts.next();
    let signature_b64 = parts.next();

    if let (Some(header_b64), Some(payload_b64), Some(signature_b64)) = (header_b64, payload_b64, signature_b64) {
        let message = format!("{}.{}", header_b64, payload_b64);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        
        if let Ok(signature) = BASE64_URL_SAFE_NO_PAD.decode(signature_b64) {
            return mac.verify_slice(&signature).is_ok();
        }
    }
    false
}
