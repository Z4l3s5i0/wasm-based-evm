use std::sync::Arc;
use crate::p2p::peer_manager::PeerManager;
use crate::p2p::rpc_client::RpcClient;
use crate::p2p::{P2pApiClient};
use crate::debug;
use jsonrpsee::core::client::ClientT;
use alloy_consensus::Block as ConsensusBlock;
use alloy_consensus::TxEnvelope as Transaction;
use alloy_rlp::Decodable;
use alloy_primitives::B256;

pub struct Downloader {
    peer_manager: Arc<PeerManager>,
}

impl Downloader {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }

    pub async fn get_best_peer(&self, local_height: u64) -> Option<(String, u64)> {
        let peers = self.peer_manager.get_active_peers().await;
        let mut best_peer = None;
        let mut best_height = local_height;

        for (peer_id, peer_info) in peers.iter() {
            let client = RpcClient::new(peer_info.p2p_url.clone());
            match client.request::<String, Vec<()>>("eth_blockNumber", vec![]).await {
                Ok(hex_num) => {
                    if let Ok(num) = u64::from_str_radix(hex_num.trim_start_matches("0x"), 16) {
                        if num > best_height {
                            best_height = num;
                            best_peer = Some((peer_info.p2p_url.clone(), num));
                        }
                    }
                }
                Err(e) => {
                    debug!("[Downloader] Failed to fetch block number from peer {}: {}", peer_id, e);
                }
            }
        }
        best_peer
    }

    pub async fn download_block(&self, rpc_url: &str, block_num: u64) -> anyhow::Result<ConsensusBlock<Transaction>> {
        let client = RpcClient::new(rpc_url.to_string());
        match client.get_block_by_number(block_num).await? {
            Some(block_rlp) => {
                let mut rlp_slice = block_rlp.as_slice();
                let block = ConsensusBlock::<Transaction>::decode(&mut rlp_slice)
                    .map_err(|e| anyhow::anyhow!("Failed to decode block: {}", e))?;
                Ok(block)
            }
            None => Err(anyhow::anyhow!("Peer returned None for block {}", block_num)),
        }
    }

    pub async fn get_block_hash(&self, rpc_url: &str, block_num: u64) -> anyhow::Result<Option<B256>> {
        let client = RpcClient::new(rpc_url.to_string());
        Ok(client.get_block_hash(block_num).await?)
    }

    pub async fn download_block_by_hash(&self, rpc_url: &str, hash: B256) -> anyhow::Result<ConsensusBlock<Transaction>> {
        let client = RpcClient::new(rpc_url.to_string());
        match client.get_block_by_hash(hash).await? {
            Some(block_rlp) => {
                let mut rlp_slice = block_rlp.as_slice();
                let block = ConsensusBlock::<Transaction>::decode(&mut rlp_slice)
                    .map_err(|e| anyhow::anyhow!("Failed to decode block: {}", e))?;
                Ok(block)
            }
            None => Err(anyhow::anyhow!("Peer returned None for block hash {:?}", hash)),
        }
    }
}
