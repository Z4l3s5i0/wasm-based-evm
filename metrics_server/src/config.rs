use crate::model::AppConfig;
use anyhow::{anyhow, Context};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;

fn validate_socket_addr(addr: &str) -> bool {
    // Try SocketAddr first (IP:port)
    if addr.parse::<SocketAddr>().is_ok() {
        return true;
    }
    // Try host:port by seeing if it has exactly one colon and the part after it is a u16
    let parts: Vec<&str> = addr.split(':').collect();
    if parts.len() == 2 {
        if let Ok(_port) = parts[1].parse::<u16>() {
            // We don't want to actually resolve it during config check, 
            // just check if it's a plausible host:port
            return !parts[0].is_empty();
        }
    }
    false
}

pub fn load_config(path: &Path) -> anyhow::Result<AppConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {:?}", path))?;
    let config: AppConfig = toml::from_str(&content)
        .with_context(|| format!("failed to parse config file: {:?}", path))?;
    Ok(config)
}

pub fn validate_config(config: &AppConfig) -> anyhow::Result<()> {
    if config.server.bind_addr.is_empty() {
        return Err(anyhow!("server.bind_addr is empty"));
    }
    config.server.bind_addr.parse::<SocketAddr>()
        .with_context(|| format!("invalid server.bind_addr: {}", config.server.bind_addr))?;

    match config.storage.kind.as_str() {
        "redb" => {
            if config.storage.path.is_empty() {
                return Err(anyhow!("storage.path is required for kind 'redb'"));
            }
        }
        "memory" => {}
        _ => return Err(anyhow!("invalid storage.kind: {} (must be 'redb' or 'memory')", config.storage.kind)),
    }

    if config.collection.interval_seconds == 0 {
        return Err(anyhow!("collection.interval_seconds must be > 0"));
    }
    if config.collection.timeout_ms == 0 {
        return Err(anyhow!("collection.timeout_ms must be > 0"));
    }
    if config.collection.max_concurrent_nodes == 0 {
        return Err(anyhow!("collection.max_concurrent_nodes must be > 0"));
    }

    // We allow starting without any static nodes because they can be registered dynamically via API.
    // However, if no nodes are provided and bootstrap is disabled, the server will just sit idle
    // until someone registers a node. This is a valid use case.

    if let Some(bootstrap) = &config.bootstrap {
        if bootstrap.enabled {
            if bootstrap.probe_interval_seconds == 0 {
                return Err(anyhow!("bootstrap.probe_interval_seconds must be > 0"));
            }
            if bootstrap.stale_after_seconds == 0 {
                return Err(anyhow!("bootstrap.stale_after_seconds must be > 0"));
            }
        }
    }

    let mut node_ids = HashSet::new();
    for node in &config.nodes {
        if node.id.is_empty() {
            return Err(anyhow!("node id cannot be empty"));
        }
        if !node_ids.insert(&node.id) {
            return Err(anyhow!("duplicate node id: {}", node.id));
        }
        if node.network.is_empty() {
            return Err(anyhow!("node network cannot be empty for node {}", node.id));
        }
        if node.client.is_empty() {
            return Err(anyhow!("node client cannot be empty for node {}", node.id));
        }
        if node.rpc_url.is_empty() {
            return Err(anyhow!("node rpc_url cannot be empty for node {}", node.id));
        }
        if !node.rpc_url.starts_with("http://") && !node.rpc_url.starts_with("https://") {
            return Err(anyhow!("node rpc_url must start with http:// or https:// for node {}", node.id));
        }
        if let Some(metrics_url) = &node.metrics_url {
            if !metrics_url.starts_with("http://") && !metrics_url.starts_with("https://") {
                return Err(anyhow!("node metrics_url must start with http:// or https:// for node {}", node.id));
            }
        }
        if let Some(p2p_addr) = &node.p2p_addr {
            if !validate_socket_addr(p2p_addr) {
                return Err(anyhow!("invalid p2p_addr for node {}: {} (expected IP:port or host:port)", node.id, p2p_addr));
            }
        }
        if let Some(discovery_addr) = &node.discovery_addr {
            if !validate_socket_addr(discovery_addr) {
                return Err(anyhow!("invalid discovery_addr for node {}: {} (expected IP:port or host:port)", node.id, discovery_addr));
            }
        }
        if let Some(enode) = &node.enode {
            if !enode.starts_with("enode://") {
                return Err(anyhow!("node enode must start with enode:// for node {}", node.id));
            }
        }
    }

    Ok(())
}
