use crate::model::{MetricSample, MetricRangeQuery, Node, Experiment, RpcObservation, CollectionError, NodeStatus};
use crate::storage::MetricsStore;
use anyhow::{Result, Context};
use redb::{Database, TableDefinition, ReadableDatabase, ReadableTable};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const EXPERIMENTS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("experiments");
const NODES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("nodes");
const METRIC_SAMPLES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("metric_samples");
const RPC_OBSERVATIONS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("rpc_observations");
const COLLECTION_ERRORS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("collection_errors");
const LATEST_METRICS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("latest_metrics");
const COUNTERS_TABLE: TableDefinition<&str, u64> = TableDefinition::new("counters");

pub struct RedbStore {
    db: Database,
    counter: AtomicU64,
}

impl RedbStore {
    pub fn new(path: &str) -> Result<Self> {
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Database::builder()
            .create(path)
            .context("failed to create redb database")?;

        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            write_txn.open_table(EXPERIMENTS_TABLE)?;
            write_txn.open_table(NODES_TABLE)?;
            write_txn.open_table(METRIC_SAMPLES_TABLE)?;
            write_txn.open_table(RPC_OBSERVATIONS_TABLE)?;
            write_txn.open_table(COLLECTION_ERRORS_TABLE)?;
            write_txn.open_table(LATEST_METRICS_TABLE)?;
            write_txn.open_table(COUNTERS_TABLE)?;
        }
        write_txn.commit()?;

        Ok(Self {
            db,
            counter: AtomicU64::new(0),
        })
    }

    fn next_id(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }
}

impl MetricsStore for RedbStore {
    fn upsert_experiment(&self, experiment: &Experiment) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(EXPERIMENTS_TABLE)?;
            let value = serde_json::to_string(experiment)?;
            table.insert(experiment.id.as_str(), value.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn upsert_node(&self, node: &Node) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(NODES_TABLE)?;
            let value = serde_json::to_string(node)?;
            table.insert(node.id.as_str(), value.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn get_node(&self, id: &str) -> Result<Option<Node>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(NODES_TABLE)?;
        if let Some(value) = table.get(id)? {
            let node: Node = serde_json::from_str(value.value())?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    fn delete_node(&self, id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(NODES_TABLE)?;
            table.remove(id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn update_node_status(
        &self,
        id: &str,
        status: NodeStatus,
        last_seen_ms: Option<i64>,
        last_successful_probe_ms: Option<i64>,
        consecutive_failures: u32,
    ) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(NODES_TABLE)?;
            let node = if let Some(existing_val) = table.get(id)? {
                let node: Node = serde_json::from_str(existing_val.value())?;
                Some(node)
            } else {
                None
            };

            if let Some(mut node) = node {
                node.status = status;
                if last_seen_ms.is_some() {
                    node.last_seen_ms = last_seen_ms;
                }
                if last_successful_probe_ms.is_some() {
                    node.last_successful_probe_ms = last_successful_probe_ms;
                }
                node.consecutive_failures = consecutive_failures;
                
                let value = serde_json::to_string(&node)?;
                table.insert(id, value.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    fn insert_rpc_observation(&self, observation: &RpcObservation) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(RPC_OBSERVATIONS_TABLE)?;
            let key = format!("{:016}:{}:{}:{}", observation.timestamp_ms, observation.node_id, observation.method, self.next_id());
            let value = serde_json::to_string(observation)?;
            table.insert(key.as_str(), value.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn insert_metric_sample(&self, sample: &MetricSample) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut samples_table = write_txn.open_table(METRIC_SAMPLES_TABLE)?;
            let key = format!("{:016}:{}:{}:{}", sample.timestamp_ms, sample.node_id, sample.name, self.next_id());
            let value = serde_json::to_string(sample)?;
            samples_table.insert(key.as_str(), value.as_str())?;

            let mut latest_table = write_txn.open_table(LATEST_METRICS_TABLE)?;
            let labels_hash = if let Some(labels) = &sample.labels_json {
                format!("{:x}", md5::compute(labels))
            } else {
                "none".to_string()
            };
            let latest_key = format!("{}:{}:{}:{}", 
                sample.experiment_id.as_deref().unwrap_or("none"),
                sample.node_id,
                sample.name,
                labels_hash
            );
            
            // Check if existing is newer
            let should_update = if let Some(existing_val) = latest_table.get(latest_key.as_str())? {
                let existing: MetricSample = serde_json::from_str::<MetricSample>(existing_val.value())?;
                sample.timestamp_ms >= existing.timestamp_ms
            } else {
                true
            };

            if should_update {
                latest_table.insert(latest_key.as_str(), value.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    fn insert_collection_error(&self, error: &CollectionError) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(COLLECTION_ERRORS_TABLE)?;
            let key = format!("{:016}:{}:{}", error.timestamp_ms, error.node_id, self.next_id());
            let value = serde_json::to_string(error)?;
            table.insert(key.as_str(), value.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn list_nodes(&self) -> Result<Vec<Node>> {
        let read_txn = ReadableDatabase::begin_read(&self.db)?;
        let table = read_txn.open_table(NODES_TABLE)?;
        let mut nodes = Vec::new();
        for item in table.iter()? {
            let item = item?;
            let value = item.1;
            let node: Node = serde_json::from_str::<Node>(value.value())?;
            nodes.push(node);
        }
        Ok(nodes)
    }

    fn list_experiments(&self) -> Result<Vec<Experiment>> {
        let read_txn = ReadableDatabase::begin_read(&self.db)?;
        let table = read_txn.open_table(EXPERIMENTS_TABLE)?;
        let mut experiments = Vec::new();
        for item in table.iter()? {
            let item = item?;
            let value = item.1;
            let exp: Experiment = serde_json::from_str::<Experiment>(value.value())?;
            experiments.push(exp);
        }
        Ok(experiments)
    }

    fn latest_metrics(&self) -> Result<Vec<MetricSample>> {
        let read_txn = ReadableDatabase::begin_read(&self.db)?;
        let table = read_txn.open_table(LATEST_METRICS_TABLE)?;
        let mut samples = Vec::new();
        for item in table.iter()? {
            let item = item?;
            let value = item.1;
            let sample: MetricSample = serde_json::from_str::<MetricSample>(value.value())?;
            samples.push(sample);
        }
        Ok(samples)
    }

    fn query_metric_range(&self, query: &MetricRangeQuery) -> Result<Vec<MetricSample>> {
        let read_txn = ReadableDatabase::begin_read(&self.db)?;
        let table = read_txn.open_table(METRIC_SAMPLES_TABLE)?;
        let mut samples = Vec::new();
        
        let start_key = format!("{:016}", query.from_ms);
        let end_key = format!("{:016}", query.to_ms + 1);

        for item in table.range(start_key.as_str()..end_key.as_str())? {
            let item = item?;
            let value = item.1;
            let sample: MetricSample = serde_json::from_str::<MetricSample>(value.value())?;
            
            if sample.name != query.metric { continue; }
            if let Some(node_id) = &query.node_id {
                if &sample.node_id != node_id { continue; }
            }
            if let Some(exp_id) = &query.experiment_id {
                if sample.experiment_id.as_ref() != Some(exp_id) { continue; }
            }

            // Label filtering
            if let Some(query_labels) = &query.labels {
                let sample_labels: std::collections::HashMap<String, serde_json::Value> = sample
                    .labels_json
                    .as_ref()
                    .and_then(|j| serde_json::from_str(j).ok())
                    .unwrap_or_default();
                
                let mut matches = true;
                for (k, v) in query_labels {
                    if sample_labels.get(k) != Some(v) {
                        matches = false;
                        break;
                    }
                }
                if !matches { continue; }
            }
            
            samples.push(sample);
        }
        Ok(samples)
    }
}
