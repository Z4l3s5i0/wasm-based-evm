use crate::model::{AppConfig, Node, NodeStatus};
use crate::storage::SharedStore;
use crate::ethereum::rpc_client::EthereumRpcClient;
use crate::ethereum::types::parse_hex_u64;
use crate::telemetry::registry::TelemetryRegistry;
use crate::time::now_ms;
use anyhow::Result;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{info, error, warn, debug};
use std::collections::HashMap;
use std::sync::Arc;

pub struct BootstrapProber {
    config: AppConfig,
    store: SharedStore,
    telemetry: TelemetryRegistry,
}

impl BootstrapProber {
    pub fn new(config: AppConfig, store: SharedStore, telemetry: TelemetryRegistry) -> Self {
        Self { config, store, telemetry }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let bootstrap_config = match &self.config.bootstrap {
            Some(c) if c.enabled => c,
            _ => {
                debug!("Bootstrap registry disabled, prober exiting");
                return Ok(());
            }
        };

        let interval_sec = bootstrap_config.probe_interval_seconds;
        let mut interval = tokio::time::interval(Duration::from_secs(interval_sec));
        
        info!("Starting bootstrap prober with {}s interval", interval_sec);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.probe_all_nodes().await {
                        error!("Error during bootstrap probing: {}", e);
                    }
                    self.update_node_counts().await;
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("Bootstrap prober received shutdown signal");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn probe_all_nodes(&self) -> Result<()> {
        let nodes = self.store.list_nodes()?;
        let timeout = Duration::from_millis(self.config.collection.timeout_ms);
        let bootstrap_config = self.config.bootstrap.as_ref().unwrap();

        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.collection.max_concurrent_nodes));
        let mut futures = Vec::new();

        for node in nodes {
            if node.status == NodeStatus::Disabled {
                continue;
            }

            let sem = semaphore.clone();
            let store = self.store.clone();
            let telemetry = self.telemetry.clone();
            let b_config = bootstrap_config.clone();

            futures.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let result = Self::probe_node_internal(&node, timeout).await;
                Self::handle_probe_result_internal(store, telemetry, node, result, &b_config)
            }));
        }

        for f in futures {
            match f.await {
                Ok(Ok(_)) => {},
                Ok(Err(e)) => error!("Probe task error: {}", e),
                Err(e) => error!("Probe task panicked: {}", e),
            }
        }

        Ok(())
    }

    pub async fn probe_node_internal(node: &Node, timeout: Duration) -> Result<u64> {
        let client = EthereumRpcClient::new(node.rpc_url.clone(), timeout)?;
        let chain_id_value = client.chain_id().await?;
        let chain_id = parse_hex_u64(&chain_id_value)?;
        Ok(chain_id)
    }

    pub fn handle_probe_result_internal(
        store: SharedStore,
        telemetry: TelemetryRegistry,
        node: Node, 
        result: Result<u64>, 
        bootstrap_config: &crate::model::BootstrapConfig
    ) -> Result<()> {
        let now = now_ms();
        let mut status = node.status.clone();
        let mut consecutive_failures = node.consecutive_failures;
        let mut last_successful_probe_ms = node.last_successful_probe_ms;

        match result {
            Ok(chain_id) => {
                let chain_match = if let Some(expected_chain_id) = bootstrap_config.chain_id {
                    chain_id == expected_chain_id
                } else {
                    true
                };

                if chain_match {
                    if status != NodeStatus::Active {
                        info!(node_id = node.id, chain_id, "Node is now ACTIVE");
                    }
                    status = NodeStatus::Active;
                    consecutive_failures = 0;
                    last_successful_probe_ms = Some(now);
                    telemetry.bootstrap_probe_total.with_label_values(&["success"]).inc();
                } else {
                    warn!(node_id = node.id, chain_id, expected = ?bootstrap_config.chain_id, "Node has wrong chain ID");
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 {
                        if status != NodeStatus::Unhealthy {
                             info!(node_id = node.id, "Node is now UNHEALTHY (wrong chain id)");
                        }
                        status = NodeStatus::Unhealthy;
                    } else {
                        status = NodeStatus::Stale;
                    }
                    telemetry.bootstrap_probe_total.with_label_values(&["wrong_chain"]).inc();
                }
            }
            Err(e) => {
                debug!("Probe failed for node {}: {}", node.id, e);
                consecutive_failures += 1;
                
                // If it was already active, don't immediately demote to pending.
                // If it's pending and failed, it stays pending or becomes unhealthy eventually.
                
                if consecutive_failures >= 3 {
                    if status != NodeStatus::Unhealthy {
                        info!(node_id = node.id, error = %e, "Node is now UNHEALTHY (probe failed)");
                    }
                    status = NodeStatus::Unhealthy;
                } else if status == NodeStatus::Active {
                    // Check if stale based on time
                    if let Some(last_success) = last_successful_probe_ms {
                        if (now - last_success) > (bootstrap_config.stale_after_seconds as i64 * 1000) {
                            if status != NodeStatus::Stale {
                                info!(node_id = node.id, "Node is now STALE (timeout)");
                            }
                            status = NodeStatus::Stale;
                        }
                    }
                }
                telemetry.bootstrap_probe_total.with_label_values(&["failure"]).inc();
            }
        }

        store.update_node_status(
            &node.id,
            status,
            Some(now),
            last_successful_probe_ms,
            consecutive_failures,
        )?;

        Ok(())
    }

    async fn update_node_counts(&self) {
        if let Ok(nodes) = self.store.list_nodes() {
            let mut counts: HashMap<(String, String), i64> = HashMap::new();
            for node in nodes {
                let status_str = match node.status {
                    NodeStatus::Pending => "pending",
                    NodeStatus::Active => "active",
                    NodeStatus::Stale => "stale",
                    NodeStatus::Unhealthy => "unhealthy",
                    NodeStatus::Disabled => "disabled",
                };
                let key = (status_str.to_string(), node.network.clone());
                *counts.entry(key).or_insert(0) += 1;
            }

            for ((status, network), count) in counts {
                self.telemetry.bootstrap_nodes.with_label_values(&[&status, &network]).set(count as f64);
            }
        }
    }
}
