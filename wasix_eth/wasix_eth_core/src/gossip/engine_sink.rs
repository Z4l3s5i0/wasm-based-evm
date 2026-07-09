use crate::{Engine, EngineEvent};
use wasix_eth_types::{async_trait, Block, Transaction};

#[async_trait]
impl EngineSink for Engine {
    async fn ingest_block(&self, block: Block<Transaction>) -> bool {
        let _ = self.event_tx.send(EngineEvent::NewBlock(block.clone()));
        true
    }
}

#[async_trait]
pub trait EngineSink: Send + Sync {
    async fn ingest_block(&self, block: Block<Transaction>) -> bool;
}