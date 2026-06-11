#[cfg(test)]
mod tests {
    use wasix_eth_app::app::App;
    use wasix_eth_app::cli::Args;
    use clap::Parser;
    use std::net::SocketAddr;
    use tempfile::tempdir;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_app_builder_setup_addresses() {
        let builder = App::builder();
        let mut args = Args::parse_from(&["wasix_eth"]);
        args.ext_ip = Some("127.0.0.1".parse().unwrap());
        args.eth_rpc_port = 8545;
        args.auth_rpc_port = 8551;
        args.p2p_port = 9002;
        args.discovery_port = 9001;

        let (eth, auth, p2p, disc) = builder.setup_addresses(&args);
        assert_eq!(eth, "127.0.0.1:8545".parse::<SocketAddr>().unwrap());
        assert_eq!(auth, "127.0.0.1:8551".parse::<SocketAddr>().unwrap());
        assert_eq!(p2p, "127.0.0.1:9002".parse::<SocketAddr>().unwrap());
        assert_eq!(disc, "127.0.0.1:9001".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn test_app_builder_setup_jwt_secret_creation() {
        let builder = App::builder();
        let tmp_dir = tempdir().unwrap();
        let data_dir = tmp_dir.path().to_path_buf();
        let mut args = Args::parse_from(&["wasix_eth"]);
        
        // Scenario 1: No JWT path provided, should create in data_dir
        let secret = builder.setup_jwt_secret(&args, &data_dir, "test_net").unwrap();
        assert!(secret.is_some());
        let jwt_path = data_dir.join("jwt_test_net.hex");
        assert!(jwt_path.exists());
        
        let content = fs::read_to_string(&jwt_path).unwrap();
        assert_eq!(content.trim().len(), 64); // 64 hex chars (no 0x prefix when generated)
    }

    #[test]
    fn test_app_builder_setup_jwt_secret_existing() {
        let builder = App::builder();
        let tmp_dir = tempdir().unwrap();
        let data_dir = tmp_dir.path().to_path_buf();
        let jwt_path = data_dir.join("my_jwt.hex");
        let secret_hex = "0x".to_string() + &"a".repeat(64);
        fs::write(&jwt_path, secret_hex).unwrap();

        let mut args = Args::parse_from(&["wasix_eth"]);
        args.auth_rpc_jwt_path = Some(jwt_path);

        let secret = builder.setup_jwt_secret(&args, &data_dir, "any").unwrap();
        assert!(secret.is_some());
        assert_eq!(secret.unwrap(), [0xaa; 32]);
    }

    #[test]
    fn test_app_builder_setup_genesis_not_found() {
        let builder = App::builder();
        let result = builder.setup_genesis(PathBuf::from("non_existent.json"));
        assert!(result.is_err());
    }
}
