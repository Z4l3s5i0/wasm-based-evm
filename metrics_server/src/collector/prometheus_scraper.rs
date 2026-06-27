use crate::model::{MetricSample, MetricSource, NodeConfig, CollectionError};
use crate::storage::SharedStore;
use crate::time::now_ms;
use anyhow::{Result, Context};
use std::time::Duration;
use std::collections::HashMap;

#[derive(Clone)]
pub struct PrometheusScraper {
    store: SharedStore,
    timeout: Duration,
    experiment_id: Option<String>,
}

impl PrometheusScraper {
    pub fn new(store: SharedStore, timeout: Duration, experiment_id: Option<String>) -> Self {
        Self { store, timeout, experiment_id }
    }

    pub async fn scrape_node(&self, node: NodeConfig) -> Result<()> {
        let url = match &node.metrics_url {
            Some(url) => url,
            None => return Ok(()),
        };

        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()?;

        let response = match client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                let error = CollectionError {
                    timestamp_ms: now_ms(),
                    experiment_id: self.experiment_id.clone(),
                    node_id: node.id.clone(),
                    source: "prometheus_scraper".to_string(),
                    message: format!("Failed to fetch metrics: {}", e),
                    recoverable: true,
                };
                self.store.insert_collection_error(&error)?;
                return Ok(());
            }
        };

        let body = response.text().await?;
        let timestamp = now_ms();

        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(sample) = self.parse_line(line, &node, timestamp) {
                self.store.insert_metric_sample(&sample)?;
            }
        }

        Ok(())
    }

    fn parse_line(&self, line: &str, node: &NodeConfig, timestamp: i64) -> Option<MetricSample> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let metric_part = parts[0];
        let value_part = parts[1];

        let value: f64 = value_part.parse().ok()?;

        let (name, labels_json) = if let Some(bracket_start) = metric_part.find('{') {
            let name = &metric_part[..bracket_start];
            let bracket_end = metric_part.find('}')?;
            let labels_str = &metric_part[bracket_start + 1..bracket_end];
            let labels = self.parse_labels(labels_str);
            let labels_json = serde_json::to_string(&labels).ok();
            (name.to_string(), labels_json)
        } else {
            (metric_part.to_string(), None)
        };

        Some(MetricSample {
            timestamp_ms: timestamp,
            experiment_id: self.experiment_id.clone(),
            node_id: node.id.clone(),
            network: node.network.clone(),
            client: node.client.clone(),
            source: MetricSource::PrometheusScrape,
            name,
            value,
            labels_json,
        })
    }

    fn parse_labels(&self, labels_str: &str) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        for pair in labels_str.split(',') {
            let kv: Vec<&str> = pair.split('=').collect();
            if kv.len() == 2 {
                let key = kv[0].trim().to_string();
                let val = kv[1].trim().trim_matches('"').to_string();
                labels.insert(key, val);
            }
        }
        labels
    }
}
