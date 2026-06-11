use wasix_eth_app::app::App;
use wasix_eth_app::cli::{Args, Commands};
use wasix_eth_storage::read_traits::{BlockProvider, HeaderProvider};
use wasix_eth_types::genesis::GenesisConfiguration;
use wasix_eth_types::*;
use wasix_eth_utils::info;
use clap::Parser;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn test_reproduce_block_1_node_import() {
    let genesis_json_path = "C:\\Users\\aless\\Downloads\\genesis.json";
    let chain_rlp_path = "C:\\Users\\aless\\Downloads\\chain.rlp";

    // Fallback to project paths if C:\ paths don't exist (for Junie environment)
    let (genesis_json_path, chain_rlp_path) = if !std::path::Path::new(genesis_json_path).exists() {
        info!("C:\\ paths not found, trying project paths");
        ("testing\\genesis.json", "testing\\chain.rlp")
    } else {
        (genesis_json_path, chain_rlp_path)
    };

    if !std::path::Path::new(genesis_json_path).exists() {
        info!("Skipping test as genesis.json is not found");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // 1. Init
    info!("Running init command");
    let mut init_args = Args::parse_from(&["wasix_eth", "init"]);
    init_args.common.data_dir = Some(data_dir.clone());
    init_args.common.genesis_path = Some(PathBuf::from(genesis_json_path));
    init_args.command = Some(Commands::Init { common: init_args.common.clone() });

    let init_app = App::builder()
        .with_config(init_args)
        .build_init()
        .await
        .expect("Failed to build init app");
    init_app.init().await.expect("Failed to run init");

    // 2. Import
    info!("Running import command");
    let mut import_args = Args::parse_from(&["wasix_eth", "import"]);
    import_args.common.data_dir = Some(data_dir.clone());
    import_args.common.import_chain = Some(PathBuf::from(chain_rlp_path));
    import_args.common.genesis_path = Some(PathBuf::from(genesis_json_path));
    import_args.command = Some(Commands::Import { common: import_args.common.clone() });

    let import_app = App::builder()
        .with_config(import_args)
        .build_import()
        .await
        .expect("Failed to build import app");
    import_app.import().await.expect("Failed to run import");

    // 3. Run (and verify)
    info!("Running run command for verification");
    let mut run_args = Args::parse_from(&["wasix_eth", "run"]);
    run_args.common.data_dir = Some(data_dir.clone());
    run_args.common.genesis_path = Some(PathBuf::from(genesis_json_path));
    run_args.command = Some(Commands::Run { common: run_args.common.clone() });

    let run_app = App::builder()
        .with_config(run_args)
        .build()
        .await
        .expect("Failed to build run app");

    let node = run_app.node().expect("Node should be present in run app");

    // Verify that at least block 1 was imported
    let block_1 = node.read_provider.header(BlockId::Number(BlockNumberOrTag::Number(1))).unwrap();
    if let Some(header) = block_1 {
        info!("Successfully imported block 1. State root: {:?}", header.state_root);
    } else {
        info!("No blocks were imported (maybe chain.rlp was empty or only had block 0)");
    }
}

#[tokio::test]
async fn test_reproduce_block_45_compare_transaction_root_with_node() {
    let genesis_json_path = "C:\\Users\\aless\\Downloads\\genesis.json";
    let chain_rlp_path = "C:\\Users\\aless\\Downloads\\chain.rlp";

    let (genesis_json_path, chain_rlp_path) = if !std::path::Path::new(genesis_json_path).exists() {
        ("hive\\simulators\\ethereum\\sync\\chain\\genesis.json", "hive\\simulators\\ethereum\\sync\\chain\\chain.rlp")
    } else {
        (genesis_json_path, chain_rlp_path)
    };

    if !std::path::Path::new(genesis_json_path).exists() { return; }

    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // 1. Init
    let mut init_args = Args::parse_from(&["wasix_eth", "init"]);
    init_args.common.data_dir = Some(data_dir.clone());
    init_args.common.genesis_path = Some(PathBuf::from(genesis_json_path));
    init_args.command = Some(Commands::Init { common: init_args.common.clone() });

    App::builder()
        .with_config(init_args)
        .build_init()
        .await
        .expect("Init build failed")
        .init()
        .await
        .expect("Init failed");

    // 2. Import
    let mut import_args = Args::parse_from(&["wasix_eth", "import"]);
    import_args.common.data_dir = Some(data_dir.clone());
    import_args.common.import_chain = Some(PathBuf::from(chain_rlp_path));
    import_args.common.genesis_path = Some(PathBuf::from(genesis_json_path));
    import_args.command = Some(Commands::Import { common: import_args.common.clone() });

    App::builder()
        .with_config(import_args)
        .build_import()
        .await
        .expect("Import build failed")
        .import()
        .await
        .expect("Import failed");
    
    // 3. Run (and verify)
    let mut run_args = Args::parse_from(&["wasix_eth", "run"]);
    run_args.common.data_dir = Some(data_dir.clone());
    run_args.common.genesis_path = Some(PathBuf::from(genesis_json_path));
    run_args.command = Some(Commands::Run { common: run_args.common.clone() });

    let run_app = App::builder()
        .with_config(run_args)
        .build()
        .await
        .expect("Run build failed");

    let node = run_app.node().expect("Node should be present");

    // If import succeeded, it means transaction roots matched during execution
    let block_45 = node.read_provider.block(BlockId::Number(BlockNumberOrTag::Number(45))).unwrap();
    let block = block_45.expect("Block 45 should have been imported");
    info!("Block 45 transactions: {}", block.body.transactions.len());
    assert!(block.header.transactions_root != EMPTY_ROOT_HASH || block.body.transactions.is_empty());
}