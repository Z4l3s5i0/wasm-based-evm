use tentacle::{
    builder::MetaBuilder,
    context::{ProtocolContext, ProtocolContextMutRef},
    service::{ProtocolHandle, ProtocolMeta},
    traits::ServiceProtocol,
    ProtocolId,
    bytes::Bytes,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::storage::InMemoryStorage;
use tracing::info;

pub const ETH_PROTOCOL_ID: ProtocolId = ProtocolId::new(1);

struct EthProtocolHandler {
    storage: Arc<Mutex<InMemoryStorage>>,
}

#[async_trait::async_trait]
impl ServiceProtocol for EthProtocolHandler {
    async fn init(&mut self, context: &mut ProtocolContext) {
        info!("EthProtocol initiated on protocol: {}", context.proto_id);
    }

    async fn connected(&mut self, context: ProtocolContextMutRef<'_>, version: &str) {
        info!("EthProtocol connected on session: {}, version: {}", context.session.id, version);
    }

    async fn disconnected(&mut self, context: ProtocolContextMutRef<'_>) {
        info!("EthProtocol disconnected on session: {}", context.session.id);
    }

    async fn received(&mut self, context: ProtocolContextMutRef<'_>, data: Bytes) {
        // Here we would decode RLP messages (Hello, Transactions, etc.)
        // For now, just log the receipt
        info!("EthProtocol received {} bytes from session: {}", data.len(), context.session.id);
    }
}

pub fn create_meta(storage: Arc<Mutex<InMemoryStorage>>) -> ProtocolMeta {
    MetaBuilder::new()
        .id(ETH_PROTOCOL_ID)
        .name(|id| format!("/eth/{}", id.value()))
        .service_handle(move || ProtocolHandle::Callback(Box::new(EthProtocolHandler { storage: storage.clone() })))
        .build()
}
