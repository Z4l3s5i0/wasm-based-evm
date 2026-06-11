use alloy_rlp::Decodable;
use wasix_eth_types::{Block, Transaction};
use wasix_eth_utils::{error, info, warn};

#[tokio::test]
async fn test_reproduce_block_1_node_import() {
    let chain_rlp_path = "C:\\Users\\aless\\Downloads\\chain.rlp";
    info!("[Node] Checking for chain.rlp at {:?}", chain_rlp_path);
    info!("[Node] Importing blocks from {:?}", chain_rlp_path);
    let data = std::fs::read(&chain_rlp_path).unwrap();
    info!("[Node] Read {} bytes from {:?}", data.len(), chain_rlp_path);
    let mut buf = &data[..];
    let mut count = 0;
    while !buf.is_empty() {
        match Block::<Transaction>::decode(&mut buf) {
            Ok(block) => {
                info!("[Node] Importing block {} (hash: {}) from chain.rlp", block.header.number, block.header.hash_slow());
                info!("[Node] Block: {:?}", block);
                count += 1;
            }
            Err(e) => {
                error!("[Node] Failed to decode block from {:?}: {}", chain_rlp_path, e);
            }
        }
    }
    info!("[Node] Imported {} blocks from chain.rlp", count);
}