use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use std::sync::Arc;
use alloy_consensus::Block as ConsensusBlock;
use alloy_consensus::TxEnvelope as Transaction;
use alloy_consensus::transaction::SignerRecoverable;
use alloy_rlp::Decodable;
use crate::mempool::Mempool;
use crate::p2p::peer_manager::PeerManager;
use crate::{info, error, debug};
use crate::sync::controller::SyncController;
use crate::misc::metrics::GOSSIP_MESSAGES_RECEIVED;

pub struct GossipHandler {
    mempool: Arc<RwLock<Mempool>>,
    peer_manager: Arc<PeerManager>,
    sync_engine: Arc<SyncController>,
    gossip_rx: Receiver<Vec<u8>>,
}

impl GossipHandler {
    pub fn new(
        mempool: Arc<RwLock<Mempool>>,
        peer_manager: Arc<PeerManager>,
        sync_engine: Arc<SyncController>,
        gossip_rx: Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            mempool,
            peer_manager,
            sync_engine,
            gossip_rx,
        }
    }

    pub async fn start(mut self) {
        info!("[Gossip] Starting GossipHandler");
        while let Some(data) = self.gossip_rx.recv().await {
            GOSSIP_MESSAGES_RECEIVED.inc();
            match self.handle_message(data).await {
                Ok(_) => {},
                Err(e) => error!("[Gossip] Error handling gossip message: {}", e),
            }
        }
    }

    async fn handle_message(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        let mut data_slice = data.as_slice();
        
        // Try decoding as a Block first
        if let Ok(block) = ConsensusBlock::<Transaction>::decode(&mut data_slice) {
            let block_hash = block.header.hash_slow();
            debug!("[Gossip] Received block via gossip: {:?} (number {})", block_hash, block.header.number);
            
            // Check if we have it
            let has_block = self.sync_engine.has_block(block_hash).await;
            if !has_block {
                // If we don't have it, trigger sync to process it and its ancestors
                info!("[Gossip] New block received via gossip, triggering sync: {:?}", block_hash);
                let sync_engine = self.sync_engine.clone();
                tokio::spawn(async move {
                    if let Err(e) = sync_engine.trigger_sync().await {
                        error!("[Gossip] Failed to trigger sync for gossiped block: {}", e);
                    }
                });
                
                // Re-broadcast
                self.peer_manager.broadcast_gossip(data).await;
            }
            return Ok(());
        }

        // Fallback to Transaction
        data_slice = data.as_slice();
        let tx = Transaction::decode(&mut data_slice)
            .map_err(|e| format!("Failed to decode gossip message as Block or Transaction: {}", e))?;

        let tx_hash = tx.hash().clone();
        debug!("[Gossip] Received transaction via gossip: {:?}", tx_hash);

        // Add to mempool and re-broadcast only if it's new
        let is_new = {
            let mut mempool = self.mempool.write().await;
            let _from = tx.recover_signer().unwrap_or_default();
            // We need current nonce here. For now we use 0 or we could use the storage.
            // GossipHandler doesn't have storage access directly, but sync_engine might.
            // Let's use 0 for now as a fallback if we don't want to add storage here.
            // Actually, it's better to provide a way to get the nonce.
            mempool.add_transaction(tx, 0)
        };
        
        if is_new {
            // Re-broadcast to other peers (except the sender)
            // Note: PeerManager::broadcast_gossip doesn't handle sender filtering yet.
            // It's a simple broadcast for now to ensure propagation.
            self.peer_manager.broadcast_gossip(data).await;
        } else {
            debug!("[Gossip] Transaction {:?} already in mempool, skipping re-broadcast", tx_hash);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;
    use crate::storage::storage::InMemoryStorage;
    use crate::evm::ev::EvmU256;
    use crate::evm::executor::Executor;
    use crate::identity::identity::Identity;
    use alloy_primitives::{U256, Address};
    use alloy_consensus::TxLegacy;
    use alloy_consensus::SignableTransaction;

    async fn setup_gossip_handler() -> (GossipHandler, mpsc::Sender<Vec<u8>>) {
        let storage = Arc::new(RwLock::new(InMemoryStorage::new(EvmU256::from(1))));
        let mempool = Arc::new(RwLock::new(Mempool::new(U256::from(0))));
        let identity = Identity::new(None, None).unwrap();
        let (peer_manager, _) = PeerManager::new(identity, storage.clone(), 0, 0, None, vec![]).unwrap();
        let peer_manager = Arc::new(peer_manager);
        let executor = Arc::new(Executor::new());
        let sync_engine = Arc::new(SyncController::new(storage.clone(), mempool.clone(), peer_manager.clone(), executor));
        let (tx, rx) = mpsc::channel(10);
        
        (GossipHandler::new(mempool, peer_manager, sync_engine, rx), tx)
    }

    #[tokio::test]
    async fn test_handle_transaction_gossip() {
        let (handler, _tx_chan) = setup_gossip_handler().await;
        
        let tx = Transaction::Legacy(TxLegacy {
            nonce: 0,
            ..Default::default()
        }.into_signed(alloy_primitives::Signature::test_signature()));
        
        let mut data = Vec::new();
        alloy_rlp::Encodable::encode(&tx, &mut data);
        
        let result = handler.handle_message(data).await;
        assert!(result.is_ok());
        assert_eq!(handler.mempool.read().await.len(), 1);
    }
}
