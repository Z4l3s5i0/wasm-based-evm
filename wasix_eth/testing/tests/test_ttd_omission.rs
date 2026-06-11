use wasix_eth_types::*;
use wasix_eth_utils::block_mapper::BlockMapper;

#[test]
fn test_total_difficulty_omission() {
    let mut config = ChainConfig::default();
    config.london_block = Some(20);
    config.terminal_total_difficulty = Some(U256::from(3_000_000));

    // A block before London
    let header_pre_london = Header {
        number: 10,
        timestamp: 100,
        difficulty: U256::from(1000),
        ..Default::default()
    };
    let block_pre_london = Block {
        header: header_pre_london,
        body: BlockBody::default(),
    };
    let td_pre_london = Some(U256::from(10_000));
    
    let rpc_block_pre = BlockMapper::to_rpc_block(block_pre_london, false, &config, td_pre_london);
    assert_eq!(rpc_block_pre.header.total_difficulty, td_pre_london);

    // A London block before TTD
    let header_london = Header {
        number: 25,
        timestamp: 200,
        difficulty: U256::from(1000),
        ..Default::default()
    };
    let block_london = Block {
        header: header_london,
        body: BlockBody::default(),
    };
    let td_london = Some(U256::from(2_000_000));
    
    let rpc_block_london = BlockMapper::to_rpc_block(block_london, false, &config, td_london);
    assert_eq!(rpc_block_london.header.total_difficulty, td_london);

    // A London block AFTER TTD reached
    let header_post_merge = Header {
        number: 27,
        timestamp: 210,
        difficulty: U256::ZERO, // Usually 0 after merge
        ..Default::default()
    };
    let block_post_merge = Block {
        header: header_post_merge,
        body: BlockBody::default(),
    };
    let td_post_merge = Some(U256::from(3_677_504)); // TD >= TTD
    
    let rpc_block_post = BlockMapper::to_rpc_block(block_post_merge, false, &config, td_post_merge);
    
    // THIS IS WHAT HIVE EXPECTS: totalDifficulty should be None
    assert_eq!(rpc_block_post.header.total_difficulty, None, "totalDifficulty should be omitted after TTD reached");
}
