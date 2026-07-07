use crate::collector::rpc_collector::RpcCollector;
use crate::collector::prometheus_scraper::PrometheusScraper;
use crate::model::{AppConfig, NodeStatus};
use crate::storage::SharedStore;
use crate::telemetry::registry::TelemetryRegistry;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Semaphore};
use tracing::{info, error};

pub struct CollectorScheduler {
    config: AppConfig,
    store: SharedStore,
    telemetry: TelemetryRegistry,
}

impl CollectorScheduler {
    pub fn new(config: AppConfig, store: SharedStore, telemetry: TelemetryRegistry) -> Self {
        Self { config, store, telemetry }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let interval_sec = self.config.collection.interval_seconds;
        let mut interval = tokio::time::interval(Duration::from_secs(interval_sec));
        
        let semaphore = Arc::new(Semaphore::new(self.config.collection.max_concurrent_nodes));
        let timeout = Duration::from_millis(self.config.collection.timeout_ms);
        let experiment_id = self.config.experiment.as_ref().map(|e| e.id.clone());

        let rpc_collector = RpcCollector::new(
            self.store.clone(),
            self.telemetry.clone(),
            timeout,
            experiment_id.clone(),
        );

        let prometheus_scraper = PrometheusScraper::new(
            self.store.clone(),
            self.telemetry.clone(),
            timeout,
            experiment_id,
        );

        info!("Starting collector scheduler with {}s interval", interval_sec);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.telemetry.collection_ticks.with_label_values::<&str>(&[]).inc();
                    
                    let nodes = match self.store.list_nodes() {
                        Ok(nodes) => nodes,
                        Err(e) => {
                            error!("Failed to list nodes from storage: {}", e);
                            continue;
                        }
                    };

                    info!("Tick: starting collection for {} nodes", nodes.len());

                    for node in nodes {
                        if node.status == NodeStatus::Disabled {
                            info!(node_id = node.id, "Skipping disabled node");
                            continue;
                        }

                        let permit = match semaphore.clone().acquire_owned().await {
                            Ok(permit) => permit,
                            Err(_) => break, // Should not happen
                        };

                        let rpc = rpc_collector.clone();
                        let scraper = prometheus_scraper.clone();
                        let node_id = node.id.clone();
                        
                        tokio::spawn(async move {
                            let _permit = permit;
                            info!(node_id = %node_id, "Collecting node metrics");
                            if let Err(e) = rpc.collect_node(node.clone()).await {
                                error!("RPC collection failed for node {}: {}", node.id, e);
                            }
                            if let Err(e) = scraper.scrape_node(node.clone()).await {
                                error!("Prometheus scrape failed for node {}: {}", node.id, e);
                            }
                            info!(node_id = %node_id, "Node metrics collection finished");
                        });
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("Collector scheduler received shutdown signal");
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}
