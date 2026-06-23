use std::error::Error;
use std::path::PathBuf;
use alloy_rlp::Decodable;
use wasix_eth_core::sync::processor::BlockProcessor;
use wasix_eth_types::{Block, Transaction};
use wasix_eth_utils::{error, info, warn};
use crate::node_components::execution::ExecutionPayload;

pub async fn import_blocks(execution: ExecutionPayload, chain_rlp: Option<PathBuf>, blocks_dir: Option<PathBuf>) -> Result<(), Box<dyn Error>> {
    info!("[Node] Starting block import process. chain_rlp: {:?}, blocks_dir: {:?}", chain_rlp, blocks_dir);
    let processor = BlockProcessor::new(execution.engine.clone());

    // 1. Import from chain.rlp if present
    if let Some(path) = chain_rlp {
        info!("[Node] Checking for chain.rlp at {:?}", path);
        if path.exists() {
            info!("[Node] Importing blocks from {:?}", path);
            let data = match std::fs::read(&path) {
                Ok(data) => data,
                Err(e) => {
                    error!("[Node] Failed to read chain.rlp at {:?}: {}", path, e);
                    return Ok(()); // Stop this source but don't block runtime
                }
            };
            info!("[Node] Read {} bytes from {:?}", data.len(), path);
            let mut buf = &data[..];
            let mut count = 0;
            while !buf.is_empty() {
                match Block::<Transaction>::decode(&mut buf) {
                    Ok(block) => {
                        let block_num = block.header.number;
                        info!("[Node] Importing block {} (hash: {}) from chain.rlp", block_num, block.header.hash_slow());
                        if let Err(e) = processor.process_block(block).await {
                            error!("[Node] Failed to import block {} from chain.rlp: {}", block_num, e);
                            continue;
                            // return Err(format!("Import failed for block {}: {}", block_num, e).into());
                        } else {
                            count += 1;
                        }
                    }
                    Err(e) => {
                        error!("[Node] Failed to decode block from {:?}: {}", path, e);
                        // continue;
                        break;
                    }
                }
            }
            info!("[Node] Imported {} blocks from chain.rlp", count);
        } else {
            warn!("[Node] chain.rlp path does not exist: {:?}", path);
        }
    }

    // 2. Import from blocks directory if present
    if let Some(dir_path) = blocks_dir {
        info!("[Node] Checking for blocks directory at {:?}", dir_path);
        if dir_path.exists() && dir_path.is_dir() {
            info!("[Node] Importing blocks from directory {:?}", dir_path);
            let mut entries: Vec<_> = match std::fs::read_dir(&dir_path) {
                Ok(dir) => dir
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "rlp"))
                    .collect(),
                Err(e) => {
                    error!("[Node] Failed to read blocks directory at {:?}: {}", dir_path, e);
                    return Ok(());
                }
            };

            info!("[Node] Found {} .rlp files in {:?}", entries.len(), dir_path);
            // Sort by filename as required by Hive
            entries.sort_by_key(|e| e.file_name());

            let mut count = 0;
            for entry in entries {
                let path = entry.path();
                info!("[Node] Importing block from {:?}", path);
                let data = match std::fs::read(&path) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("[Node] Failed to read block file at {:?}: {}", path, e);
                        break;
                    }
                };
                let mut buf = &data[..];
                match Block::<Transaction>::decode(&mut buf) {
                    Ok(block) => {
                        let block_num = block.header.number;
                        info!("[Node] Importing block {} (hash: {}) from {:?}", block_num, block.header.hash_slow(), path);
                        if let Err(e) = processor.process_block(block).await {
                            error!("[Node] Failed to import block {} from {:?}: {}", block_num, path, e);
                            // return Err(format!("Import failed for block {} from {:?}: {}", block_num, path, e).into());
                            continue;
                        } else {
                            count += 1;
                        }
                    }
                    Err(e) => {
                        error!("[Node] Failed to decode block from {:?}: {}", path, e);
                        break;
                    }
                }
            }
            info!("[Node] Imported {} blocks from directory", count);
        } else if !dir_path.exists() {
            warn!("[Node] blocks directory does not exist: {:?}", dir_path);
        }
    }

    info!("[Node] Block import process completed");
    Ok(())
}