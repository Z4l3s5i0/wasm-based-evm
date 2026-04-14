use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use std::sync::Arc;
use alloy_consensus::Block as ConsensusBlock;
use alloy_consensus::TxEnvelope as Transaction;
use alloy_rlp::Decodable;
use crate::mempool::Mempool;
use crate::p2p::peer_manager::PeerManager;
use crate::p2p::sync::SyncEngine;
use crate::{info, error, debug};

pub struct GossipHandler {
    mempool: Arc<RwLock<Mempool>>,
    peer_manager: Arc<PeerManager>,
    sync_engine: Arc<SyncEngine>,
    gossip_rx: Receiver<Vec<u8>>,
}

impl GossipHandler {
    pub fn new(
        mempool: Arc<RwLock<Mempool>>,
        peer_manager: Arc<PeerManager>,
        sync_engine: Arc<SyncEngine>,
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
