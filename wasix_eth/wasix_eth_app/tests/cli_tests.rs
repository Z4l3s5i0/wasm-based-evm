#[cfg(test)]
mod tests {
    use wasix_eth_app::cli::Args;
    use clap::Parser;
    use std::net::IpAddr;
    use std::path::PathBuf;
    use std::str::FromStr;

    #[test]
    fn test_args_default_values() {
        let args = Args::parse_from(&["wasix_eth"]);
        assert_eq!(args.discovery_port, 9001);
        assert_eq!(args.p2p_port, 9002);
        assert_eq!(args.eth_rpc_port, 8545);
        assert_eq!(args.auth_rpc_port, 8551);
        assert_eq!(args.frontend_port, 3000);
        assert!(args.bootnodes.is_empty());
        assert_eq!(args.max_peers, 50);
        assert!(args.ext_ip.is_none());
        assert!(args.data_dir.is_none());
        assert!(args.genesis_path.is_none());
        assert_eq!(args.chain, "devnet");
        assert!(args.auth_rpc_jwt_path.is_none());
        assert_eq!(args.verbose, 1);
        assert!(args.dev.is_none());
        assert!(args.peer_name.is_none());
        assert_eq!(args.metrics_port, 9050);
        assert!(args.import_chain.is_none());
        assert!(args.import_blocks.is_none());
    }

    #[test]
    fn test_args_custom_values() {
        let args = Args::parse_from(&[
            "wasix_eth",
            "--discovery-port", "10001",
            "--p2p-port", "10002",
            "--eth-rpc-port", "18545",
            "--auth-rpc-port", "18551",
            "--frontend-port", "4000",
            "--bootnodes", "enode://abc@127.0.0.1:9001,enode://def@127.0.0.1:9002",
            "--max-peers", "100",
            "--ext-ip", "1.2.3.4",
            "--data-dir", "/tmp/data",
            "--genesis-path", "/tmp/genesis.json",
            "--chain", "mainnet",
            "--auth-rpc-jwt-path", "/tmp/jwt.hex",
            "--verbose", "2",
            "--dev", "5",
            "--peer-name", "test-node",
            "--metrics-port", "19050",
            "--import-chain", "/tmp/chain.rlp",
            "--import-blocks", "/tmp/blocks"
        ]);

        assert_eq!(args.discovery_port, 10001);
        assert_eq!(args.p2p_port, 10002);
        assert_eq!(args.eth_rpc_port, 18545);
        assert_eq!(args.auth_rpc_port, 18551);
        assert_eq!(args.frontend_port, 4000);
        assert_eq!(args.bootnodes, vec!["enode://abc@127.0.0.1:9001", "enode://def@127.0.0.1:9002"]);
        assert_eq!(args.max_peers, 100);
        assert_eq!(args.ext_ip, Some(IpAddr::from_str("1.2.3.4").unwrap()));
        assert_eq!(args.data_dir, Some(PathBuf::from("/tmp/data")));
        assert_eq!(args.genesis_path, Some(PathBuf::from("/tmp/genesis.json")));
        assert_eq!(args.chain, "mainnet");
        assert_eq!(args.auth_rpc_jwt_path, Some(PathBuf::from("/tmp/jwt.hex")));
        assert_eq!(args.verbose, 2);
        assert_eq!(args.dev, Some(5));
        assert_eq!(args.peer_name, Some("test-node".to_string()));
        assert_eq!(args.metrics_port, 19050);
        assert_eq!(args.import_chain, Some(PathBuf::from("/tmp/chain.rlp")));
        assert_eq!(args.import_blocks, Some(PathBuf::from("/tmp/blocks")));
    }

    #[test]
    fn test_args_dev_flag_only() {
        let args = Args::parse_from(&["wasix_eth", "--dev"]);
        assert_eq!(args.dev, Some(12));
    }
}