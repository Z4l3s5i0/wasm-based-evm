#[cfg(test)]
mod tests {
    use wasix_eth_app::app::App;
    use wasix_eth_app::cli::{Args, Commands};
    use wasix_eth_types::genesis::GenesisConfiguration;
    use clap::Parser;
    use tempfile::tempdir;
    use std::fs;

    #[tokio::test]
    async fn test_run_works_without_genesis_path_if_initialized() {
        let tmp_dir = tempdir().unwrap();
        let data_dir = tmp_dir.path().to_path_buf();
        
        // Setup a dummy genesis file
        let genesis_path = data_dir.join("genesis.json");
        let genesis_config = GenesisConfiguration::default();
        fs::write(&genesis_path, serde_json::to_string(&genesis_config).unwrap()).unwrap();

        // 1. Run Init
        {
            let mut init_args = Args::parse_from(&["wasix_eth", "init"]);
            init_args.common.data_dir = Some(data_dir.clone());
            init_args.common.genesis_path = Some(genesis_path.clone());
            init_args.common.peer_name = Some("test_node".to_string());
            init_args.command = Some(Commands::Init { common: init_args.common.clone() });

            let init_builder = App::builder().with_config(init_args);
            let init_app = init_builder.build_init().await.expect("build_init failed");
            init_app.init().await.expect("init failed");
        }
        
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 2. Run without genesis path
        let mut run_args = Args::parse_from(&["wasix_eth", "run"]);
        run_args.common.data_dir = Some(data_dir.clone());
        run_args.common.peer_name = Some("test_node".to_string());
        run_args.common.genesis_path = None; // Explicitly none
        run_args.command = Some(Commands::Run { common: run_args.common.clone() });

        let run_builder = App::builder().with_config(run_args);
        let result = run_builder.build().await;

        assert!(result.is_ok(), "Expected build to succeed without genesis_path if already initialized: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_run_fails_without_genesis_path_if_not_initialized() {
        let tmp_dir = tempdir().unwrap();
        let data_dir = tmp_dir.path().to_path_buf();
        
        // Run without genesis path and without prior init
        let mut run_args = Args::parse_from(&["wasix_eth", "run"]);
        run_args.common.data_dir = Some(data_dir.clone());
        run_args.common.peer_name = Some("test_node".to_string());
        run_args.common.genesis_path = None;
        run_args.command = Some(Commands::Run { common: run_args.common.clone() });

        let run_builder = App::builder().with_config(run_args);
        let result = run_builder.build().await;

        assert!(result.is_err(), "Expected build to fail without genesis_path if NOT initialized");
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("Chain config not found in database"), "Error message should mention missing chain config. Got: {}", err_msg);
    }
}
