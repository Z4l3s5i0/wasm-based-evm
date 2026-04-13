use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use std::sync::Arc;
use alloy_consensus::TxEnvelope as Transaction;
use alloy_rlp::Decodable;
use crate::mempool::Mempool;
use crate::p2p::peer_manager::PeerManager;
use crate::{info, error, debug};

pub struct GossipHandler {
    mempool: Arc<RwLock<Mempool>>,
    peer_manager: Arc<PeerManager>,
    gossip_rx: Receiver<Vec<u8>>,
}

impl GossipHandler {
    pub fn new(
        mempool: Arc<RwLock<Mempool>>,
        peer_manager: Arc<PeerManager>,
        gossip_rx: Receiver<Vec<u8>>,
    ) -> Self {
        Self {
            mempool,
            peer_manager,
            gossip_rx,
        }
    }

    pub async fn start(mut self) {
        info!("[Gossip] Starting GossipHandler");
        while let Some(data) = self.gossip_rx.recv().await {
            match self.handle_message(data).await {
                Ok(_) => {},
                Err(e) => error!("[Gossip] Error handling gossip message: {}", e),
            }
        }
    }

    async fn handle_message(&self, data: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        // Deserialize as an Ethereum Transaction (RLP encoded TxEnvelope)
        let mut data_slice = data.as_slice();
        let tx = Transaction::decode(&mut data_slice)
            .map_err(|e| format!("Failed to decode transaction: {}", e))?;

        let tx_hash = tx.hash().clone();
        debug!("[Gossip] Received transaction via gossip: {:?}", tx_hash);

        // Add to mempool and re-broadcast only if it's new
        let is_new = {
            let mut mempool = self.mempool.write().await;
            mempool.add_transaction(tx)
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
