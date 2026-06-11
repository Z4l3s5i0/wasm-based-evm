#[cfg(test)]
mod tests {
    use wasix_eth_app::app::App;
    use wasix_eth_app::cli::{Args, Commands};
    use wasix_eth_types::genesis::GenesisConfiguration;
    use clap::Parser;
    use tempfile::tempdir;
    use std::fs;

    #[tokio::test]
    async fn test_import_fails_without_init() {
        let tmp_dir = tempdir().unwrap();
        let data_dir = tmp_dir.path().to_path_buf();
        
        // Setup a dummy genesis file
        let genesis_path = data_dir.join("genesis.json");
        let genesis_config = GenesisConfiguration::default();
        fs::write(&genesis_path, serde_json::to_string(&genesis_config).unwrap()).unwrap();

        // Prepare args for import
        let mut args = Args::parse_from(&["wasix_eth", "import"]);
        args.common.data_dir = Some(data_dir.clone());
        args.common.genesis_path = Some(genesis_path.clone());
        args.common.peer_name = Some("test_node".to_string());
        args.command = Some(Commands::Import { common: args.common.clone() });

        // Attempt to build app for import
        let builder = App::builder().with_config(args);
        let result = builder.build_import().await;

        // It should fail because init was not called
        assert!(result.is_err(), "Expected build_import to fail when genesis is not initialized");
        match result {
            Err(e) => {
                let err_msg = e.to_string();
                assert!(err_msg.contains("Genesis must be initialized before importing blocks"), "Error message should mention genesis initialization. Got: {}", err_msg);
            }
            Ok(_) => unreachable!(),
        }
    }

    #[tokio::test]
    async fn test_import_succeeds_after_init() {
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
            // init_app dropped here
        }
        // Small sleep to ensure file lock is released (especially on Windows)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 2. Run Import
        let mut import_args = Args::parse_from(&["wasix_eth", "import"]);
        import_args.common.data_dir = Some(data_dir.clone());
        import_args.common.peer_name = Some("test_node".to_string());
        import_args.command = Some(Commands::Import { common: import_args.common.clone() });

        let import_builder = App::builder().with_config(import_args);
        let result = import_builder.build_import().await;

        // It should succeed because init was called
        assert!(result.is_ok(), "Expected build_import to succeed after genesis is initialized: {:?}", result.err());
    }
}
