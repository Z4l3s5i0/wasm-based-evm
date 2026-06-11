use wasix_eth_types::{ChainConfig, Hardfork, calc_blob_gasprice};
use serde_json::json;

#[test]
fn test_parse_blob_schedule() {
    let genesis_json = json!({
        "chainId": 35039958,
        "homesteadBlock": 0,
        "eip150Block": 3,
        "eip155Block": 6,
        "eip158Block": 6,
        "byzantiumBlock": 9,
        "constantinopleBlock": 12,
        "petersburgBlock": 15,
        "istanbulBlock": 18,
        "muirGlacierBlock": 21,
        "berlinBlock": 24,
        "londonBlock": 27,
        "arrowGlacierBlock": 30,
        "grayGlacierBlock": 33,
        "mergeNetsplitBlock": 36,
        "shanghaiTime": 390,
        "cancunTime": 420,
        "pragueTime": 450,
        "terminalTotalDifficulty": 4732736,
        "blobSchedule": {
          "cancun": {
            "target": 3,
            "max": 6,
            "baseFeeUpdateFraction": 3338477
          },
          "prague": {
            "target": 6,
            "max": 9,
            "baseFeeUpdateFraction": 5007716
          }
        }
    });

    let config: ChainConfig = serde_json::from_value(genesis_json).expect("Failed to parse ChainConfig");
    
    // Check Cancun params
    let cancun_params = Hardfork::Cancun.blob_params(&config).expect("Cancun params missing");
    assert_eq!(cancun_params.target_blob_count, 3);
    assert_eq!(cancun_params.max_blob_count, 6);
    assert_eq!(cancun_params.update_fraction, 3338477);

    // Check Prague params
    let prague_params = Hardfork::Prague.blob_params(&config).expect("Prague params missing");
    assert_eq!(prague_params.target_blob_count, 6);
    assert_eq!(prague_params.max_blob_count, 9);
    assert_eq!(prague_params.update_fraction, 5007716);

    // Test gas price calculation with custom fraction
    let excess = 131072 * 100; // Many blobs over target to ensure price increase
    let price = calc_blob_gasprice(excess, prague_params.update_fraction);
    println!("Price: {}", price);
    assert!(price > 1);
}
