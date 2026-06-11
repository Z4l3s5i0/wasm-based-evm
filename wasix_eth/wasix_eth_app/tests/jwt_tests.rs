#[cfg(test)]
mod tests {
    use wasix_eth_app::jwt::{validate_jwt, JwtAuthLayer, HeaderInjectorLayer};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use base64::prelude::BASE64_URL_SAFE_NO_PAD;
    use base64::Engine;
    use tower::{Service, Layer};
    use http::{Request, Response, HeaderMap, header};
    use jsonrpsee::types::{Request as RpcRequest, Id};
    use jsonrpsee::server::middleware::rpc::{RpcServiceT, MethodResponse};
    use jsonrpsee::core::middleware::{Batch, Notification};
    use jsonrpsee::ResponsePayload;
    use std::future::Future;

    #[test]
    fn test_validate_jwt_valid() {
        let secret = [1u8; 32];
        let header = r#"{"alg":"HS256","typ":"JWT"}"#;
        let payload = r#"{"iat":1234567890}"#;
        
        let header_b64 = BASE64_URL_SAFE_NO_PAD.encode(header);
        let payload_b64 = BASE64_URL_SAFE_NO_PAD.encode(payload);
        
        let message = format!("{}.{}", header_b64, payload_b64);
        let mut mac = Hmac::<Sha256>::new_from_slice(&secret).unwrap();
        mac.update(message.as_bytes());
        let signature = mac.finalize().into_bytes();
        let signature_b64 = BASE64_URL_SAFE_NO_PAD.encode(signature);
        
        let token = format!("{}.{}.{}", header_b64, payload_b64, signature_b64);
        
        assert!(validate_jwt(&token, &secret));
    }

    #[test]
    fn test_validate_jwt_invalid_signature() {
        let secret = [1u8; 32];
        let wrong_secret = [2u8; 32];
        let token = create_token("payload", &secret);
        assert!(!validate_jwt(&token, &wrong_secret));
    }

    #[test]
    fn test_validate_jwt_malformed() {
        let secret = [1u8; 32];
        assert!(!validate_jwt("not.a.token", &secret));
        assert!(!validate_jwt("one.two.three.four", &secret));
    }

    fn create_token(payload: &str, secret: &[u8; 32]) -> String {
        let header = r#"{"alg":"HS256","typ":"JWT"}"#;
        let header_b64 = BASE64_URL_SAFE_NO_PAD.encode(header);
        let payload_b64 = BASE64_URL_SAFE_NO_PAD.encode(payload);
        let message = format!("{}.{}", header_b64, payload_b64);
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(message.as_bytes());
        let signature = mac.finalize().into_bytes();
        let signature_b64 = BASE64_URL_SAFE_NO_PAD.encode(signature);
        format!("{}.{}.{}", header_b64, payload_b64, signature_b64)
    }

    #[tokio::test]
    async fn test_header_injector_layer() {
        #[derive(Clone)]
        struct MockService;
        impl Service<Request<String>> for MockService {
            type Response = Response<String>;
            type Error = std::convert::Infallible;
            type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

            fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
                std::task::Poll::Ready(Ok(()))
            }

            fn call(&mut self, req: Request<String>) -> Self::Future {
                let headers = req.extensions().get::<HeaderMap>().expect("Headers should be injected");
                assert!(headers.contains_key("x-test-header"));
                std::future::ready(Ok(Response::new("ok".to_string())))
            }
        }

        let mut svc = HeaderInjectorLayer.layer(MockService);
        let mut req = Request::new("body".to_string());
        req.headers_mut().insert("x-test-header", header::HeaderValue::from_static("test"));
        
        let _ = svc.call(req).await.unwrap();
    }

    #[tokio::test]
    async fn test_jwt_auth_layer_bypass_non_engine() {
        #[derive(Clone)]
        struct MockRpcService;
        impl RpcServiceT for MockRpcService {
            type MethodResponse = MethodResponse;
            type NotificationResponse = ();
            type BatchResponse = ();

            fn call<'a>(&self, req: RpcRequest<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
                async move { MethodResponse::response(req.id().into_owned(), ResponsePayload::success("ok"), 0) }
            }
            fn batch<'a>(&self, _reqs: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a { async move { () } }
            fn notification<'a>(&self, _n: Notification<'a>) -> impl Future<Output = Self::NotificationResponse> + Send + 'a { async move { () } }
        }

        let secret = [1u8; 32];
        let layer = JwtAuthLayer::new(secret);
        let svc = layer.layer(MockRpcService);
        
        let mut req = RpcRequest::borrowed("eth_blockNumber", None, Id::Number(1));
        req.extensions_mut().insert(HeaderMap::new());
        let res = svc.call(req).await;
        assert!(!res.is_error());
    }

    #[tokio::test]
    async fn test_jwt_auth_layer_engine_valid_auth() {
        #[derive(Clone)]
        struct MockRpcService;
        impl RpcServiceT for MockRpcService {
            type MethodResponse = MethodResponse;
            type NotificationResponse = ();
            type BatchResponse = ();
            fn call<'a>(&self, req: RpcRequest<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
                async move { MethodResponse::response(req.id().into_owned(), ResponsePayload::success("ok"), 0) }
            }
            fn batch<'a>(&self, _reqs: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a { async move { () } }
            fn notification<'a>(&self, _n: Notification<'a>) -> impl Future<Output = Self::NotificationResponse> + Send + 'a { async move { () } }
        }

        let secret = [1u8; 32];
        let layer = JwtAuthLayer::new(secret);
        let svc = layer.layer(MockRpcService);
        
        let mut req = RpcRequest::borrowed("engine_newPayloadV1", None, Id::Number(1));
        let mut headers = HeaderMap::new();
        let token = create_token("{}", &secret);
        headers.insert(header::AUTHORIZATION, header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
        req.extensions_mut().insert(headers);
        
        let res = svc.call(req).await;
        assert!(!res.is_error());
    }

    #[tokio::test]
    async fn test_jwt_auth_layer_engine_invalid_auth() {
        #[derive(Clone)]
        struct MockRpcService;
        impl RpcServiceT for MockRpcService {
            type MethodResponse = MethodResponse;
            type NotificationResponse = ();
            type BatchResponse = ();
            fn call<'a>(&self, req: RpcRequest<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
                async move { MethodResponse::response(req.id().into_owned(), ResponsePayload::success("ok"), 0) }
            }
            fn batch<'a>(&self, _reqs: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a { async move { () } }
            fn notification<'a>(&self, _n: Notification<'a>) -> impl Future<Output = Self::NotificationResponse> + Send + 'a { async move { () } }
        }

        let secret = [1u8; 32];
        let layer = JwtAuthLayer::new(secret);
        let svc = layer.layer(MockRpcService);
        
        let mut req = RpcRequest::borrowed("engine_newPayloadV1", None, Id::Number(1));
        let mut headers = HeaderMap::new();
        let token = create_token("{}", &[2u8; 32]); // Wrong secret
        headers.insert(header::AUTHORIZATION, header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());
        req.extensions_mut().insert(headers);
        
        let res = svc.call(req).await;
        assert!(res.is_error());
    }

    #[tokio::test]
    async fn test_jwt_auth_layer_engine_invalid_header_format() {
        #[derive(Clone)]
        struct MockRpcService;
        impl RpcServiceT for MockRpcService {
            type MethodResponse = MethodResponse;
            type NotificationResponse = ();
            type BatchResponse = ();
            fn call<'a>(&self, req: RpcRequest<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
                async move { MethodResponse::response(req.id().into_owned(), ResponsePayload::success("ok"), 0) }
            }
            fn batch<'a>(&self, _reqs: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a { async move { () } }
            fn notification<'a>(&self, _n: Notification<'a>) -> impl Future<Output = Self::NotificationResponse> + Send + 'a { async move { () } }
        }

        let secret = [1u8; 32];
        let layer = JwtAuthLayer::new(secret);
        let svc = layer.layer(MockRpcService);
        
        let mut req = RpcRequest::borrowed("engine_newPayloadV1", None, Id::Number(1));
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, header::HeaderValue::from_static("Basic abc"));
        req.extensions_mut().insert(headers);
        
        let res = svc.call(req).await;
        assert!(res.is_error());
    }

    #[tokio::test]
    async fn test_jwt_auth_layer_engine_missing_auth() {
        #[derive(Clone)]
        struct MockRpcService;
        impl RpcServiceT for MockRpcService {
            type MethodResponse = MethodResponse;
            type NotificationResponse = ();
            type BatchResponse = ();
            fn call<'a>(&self, req: RpcRequest<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
                async move { MethodResponse::response(req.id().into_owned(), ResponsePayload::success("ok"), 0) }
            }
            fn batch<'a>(&self, _reqs: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a { async move { () } }
            fn notification<'a>(&self, _n: Notification<'a>) -> impl Future<Output = Self::NotificationResponse> + Send + 'a { async move { () } }
        }

        let secret = [1u8; 32];
        let layer = JwtAuthLayer::new(secret);
        let svc = layer.layer(MockRpcService);
        
        let req = RpcRequest::borrowed("engine_newPayloadV1", None, Id::Number(1));
        let res = svc.call(req).await;
        assert!(res.is_error());
    }
}
