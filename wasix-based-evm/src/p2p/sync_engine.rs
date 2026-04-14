use std::sync::Arc;
use tokio::sync::RwLock;
use alloy_consensus::Block as ConsensusBlock;
use alloy_consensus::TxEnvelope as Transaction;
use alloy_rlp::Decodable;
use crate::storage::storage::InMemoryStorage;
use crate::p2p::peer_manager::PeerManager;
use crate::p2p::rpc_client::RpcClient;
use crate::p2p::P2pApiClient;
use crate::{info, error, debug};
use jsonrpsee::core::client::ClientT;

pub struct SyncEngine {
    storage: Arc<RwLock<InMemoryStorage>>,
    peer_manager: Arc<PeerManager>,
}

impl SyncEngine {
    pub fn new(storage: Arc<RwLock<InMemoryStorage>>, peer_manager: Arc<PeerManager>) -> Self {
        Self {
            storage,
            peer_manager,
        }
    }

    pub async fn start(&self) {
        info!("[Sync] Starting synchronization engine...");
        loop {
            if let Err(e) = self.sync_step().await {
                error!("[Sync] Sync step failed: {}", e);
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
    }

    async fn sync_step(&self) -> Result<(), Box<dyn std::error::Error>> {
        let peers = self.peer_manager.get_active_peers().await;
        if peers.is_empty() {
            debug!("[Sync] No active peers for synchronization");
            return Ok(());
        }

        let local_height = {
            let storage_read = self.storage.read().await;
            storage_read.get_latest_block_number()
        };

        let mut best_peer = None;
        let mut best_height = local_height;

        for (peer_id, peer_info) in peers {
            let client = RpcClient::new(peer_info.p2p_url.clone());
            // Standard eth_blockNumber returns 0x...
            match client.request::<String, Vec<()>>("eth_blockNumber", vec![]).await {
                Ok(hex_num) => {
                    let num = u64::from_str_radix(hex_num.trim_start_matches("0x"), 16)?;
                    if num > best_height {
                        best_height = num;
                        best_peer = Some((peer_id, peer_info.p2p_url));
                    }
                }
                Err(e) => {
                    debug!("[Sync] Failed to fetch block number from peer {}: {}", peer_id, e);
                }
            }
        }

        if let Some((peer_id, rpc_url)) = best_peer {
            info!("[Sync] Syncing from peer {} (height: {} -> {})", peer_id, local_height, best_height);
            let client = RpcClient::new(rpc_url);
            
            for next_block_num in (local_height + 1)..=best_height {
                debug!("[Sync] Requesting block {} from peer {}", next_block_num, peer_id);
                
                match client.get_block_by_number(next_block_num).await {
                    Ok(Some(block_rlp)) => {
                        let mut rlp_slice = block_rlp.as_slice();
                        match ConsensusBlock::<Transaction>::decode(&mut rlp_slice) {
                            Ok(block) => {
                                let mut storage_write = self.storage.write().await;
                                storage_write.add_block(block);
                                info!("[Sync] Imported block {} from peer {}", next_block_num, peer_id);
                            }
                            Err(e) => {
                                error!("[Sync] Failed to decode block {} from peer {}: {}", next_block_num, peer_id, e);
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        error!("[Sync] Peer {} returned None for block {}", peer_id, next_block_num);
                        break;
                    }
                    Err(e) => {
                        error!("[Sync] Failed to download block {} from peer {}: {}", next_block_num, peer_id, e);
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}
