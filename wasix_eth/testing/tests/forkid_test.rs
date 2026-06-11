use wasix_eth_types::p2p::ForkId;
use wasix_eth_types::{B256, ChainConfig};

#[test]
fn test_forkid_paris_genesis() {
    let genesis_hash = "34cb47b1a70a73ad1e455e97f33827d94284f5e7b819f4132e466cf3cd0a0d56".parse::<B256>().unwrap();
    let mut config = ChainConfig::default();
    config.homestead_block = Some(0);
    config.eip150_block = Some(0);
    config.eip155_block = Some(0);
    config.eip158_block = Some(0);
    config.byzantium_block = Some(0);
    config.constantinople_block = Some(0);
    config.petersburg_block = Some(0);
    config.istanbul_block = Some(0);
    config.muir_glacier_block = Some(0);
    config.berlin_block = Some(0);
    config.london_block = Some(0);
    config.merge_netsplit_block = Some(0);
    
    let fork_id = ForkId::new(genesis_hash, &config, 0, 0, 0);
    
    let expected_hash = [0x9e, 0xf6, 0x77, 0x7b];
    assert_eq!(fork_id.hash, expected_hash, "Have {:x?}, want {:x?}", fork_id.hash, expected_hash);
    assert_eq!(fork_id.next, 0);
}

#[test]
fn test_forkid_validate_compatibility() {
    let genesis_hash = "34cb47b1a70a73ad1e455e97f33827d94284f5e7b819f4132e466cf3cd0a0d56".parse::<B256>().unwrap();
    let mut config = ChainConfig::default();
    config.homestead_block = Some(0);
    config.london_block = Some(0);
    config.merge_netsplit_block = Some(0);
    
    let fork_id = ForkId::new(genesis_hash, &config, 0, 0, 0);
    
    // Remote sends the same ForkId
    let res = fork_id.validate(genesis_hash, &config, 0, 0, 0);
    assert!(res.is_ok(), "Validation failed: {:?}", res.err());
}

#[test]
fn test_forkid_with_future_fork() {
    let genesis_hash = "34cb47b1a70a73ad1e455e97f33827d94284f5e7b819f4132e466cf3cd0a0d56".parse::<B256>().unwrap();
    let mut config = ChainConfig::default();
    config.homestead_block = Some(0);
    config.london_block = Some(100);
    
    // head at 0
    let fork_id = ForkId::new(genesis_hash, &config, 0, 0, 0);
    assert_eq!(fork_id.hash, [0x9e, 0xf6, 0x77, 0x7b]);
    assert_eq!(fork_id.next, 100);
    
    // head at 100
    let fork_id_100 = ForkId::new(genesis_hash, &config, 100, 0, 0);
    assert_ne!(fork_id_100.hash, [0x9e, 0xf6, 0x77, 0x7b]);
    assert_eq!(fork_id_100.next, 0);
}

#[test]
fn test_eth_message_id_mapping() {
    use wasix_eth_types::p2p::{EthMessageID, EthVersion};
    
    // eth/66
    assert_eq!(EthMessageID::GetReceipts.to_u8(), 0x0f);
    assert_eq!(EthMessageID::Receipts.to_u8(), 0x10);
    assert_eq!(EthMessageID::message_count(EthVersion::Eth66), 17);
    
    // eth/67
    assert_eq!(EthMessageID::GetReceipts.to_u8(), 0x0f);
    assert_eq!(EthMessageID::Receipts.to_u8(), 0x10);
    assert_eq!(EthMessageID::message_count(EthVersion::Eth67), 17);
    
    // eth/68
    assert_eq!(EthMessageID::GetReceipts.to_u8(), 0x0f);
    assert_eq!(EthMessageID::Receipts.to_u8(), 0x10);
    assert_eq!(EthMessageID::message_count(EthVersion::Eth68), 17);
}
