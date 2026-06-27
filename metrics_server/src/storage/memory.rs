use crate::model::{MetricSample, MetricRangeQuery, Node, Experiment, RpcObservation, CollectionError};
use crate::storage::MetricsStore;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::RwLock;

pub struct MemoryStore {
    experiments: RwLock<HashMap<String, Experiment>>,
    nodes: RwLock<HashMap<String, Node>>,
    rpc_observations: RwLock<Vec<RpcObservation>>,
    metric_samples: RwLock<Vec<MetricSample>>,
    collection_errors: RwLock<Vec<CollectionError>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            experiments: RwLock::new(HashMap::new()),
            nodes: RwLock::new(HashMap::new()),
            rpc_observations: RwLock::new(Vec::new()),
            metric_samples: RwLock::new(Vec::new()),
            collection_errors: RwLock::new(Vec::new()),
        }
    }
}

impl MetricsStore for MemoryStore {
    fn upsert_experiment(&self, experiment: &Experiment) -> Result<()> {
        let mut experiments = self.experiments.write().unwrap();
        experiments.insert(experiment.id.clone(), experiment.clone());
        Ok(())
    }

    fn upsert_node(&self, node: &Node) -> Result<()> {
        let mut nodes = self.nodes.write().unwrap();
        nodes.insert(node.id.clone(), node.clone());
        Ok(())
    }

    fn insert_rpc_observation(&self, observation: &RpcObservation) -> Result<()> {
        let mut observations = self.rpc_observations.write().unwrap();
        observations.push(observation.clone());
        Ok(())
    }

    fn insert_metric_sample(&self, sample: &MetricSample) -> Result<()> {
        let mut samples = self.metric_samples.write().unwrap();
        samples.push(sample.clone());
        Ok(())
    }

    fn insert_collection_error(&self, error: &CollectionError) -> Result<()> {
        let mut errors = self.collection_errors.write().unwrap();
        errors.push(error.clone());
        Ok(())
    }

    fn list_nodes(&self) -> Result<Vec<Node>> {
        let nodes = self.nodes.read().unwrap();
        Ok(nodes.values().cloned().collect())
    }

    fn list_experiments(&self) -> Result<Vec<Experiment>> {
        let experiments = self.experiments.read().unwrap();
        Ok(experiments.values().cloned().collect())
    }

    fn latest_metrics(&self) -> Result<Vec<MetricSample>> {
        let samples = self.metric_samples.read().unwrap();
        let mut latest: HashMap<String, MetricSample> = HashMap::new();

        for sample in samples.iter() {
            let key = format!(
                "{:?}:{}:{}:{:?}",
                sample.experiment_id, sample.node_id, sample.name, sample.labels_json
            );
            
            if let Some(existing) = latest.get(&key) {
                if sample.timestamp_ms > existing.timestamp_ms {
                    latest.insert(key, sample.clone());
                }
            } else {
                latest.insert(key, sample.clone());
            }
        }

        Ok(latest.into_values().collect())
    }

    fn query_metric_range(&self, query: &MetricRangeQuery) -> Result<Vec<MetricSample>> {
        let samples = self.metric_samples.read().unwrap();
        let filtered = samples.iter().filter(|s| {
            if s.name != query.metric { return false; }
            if let Some(node_id) = &query.node_id {
                if &s.node_id != node_id { return false; }
            }
            if let Some(exp_id) = &query.experiment_id {
                if s.experiment_id.as_ref() != Some(exp_id) { return false; }
            }
            if s.timestamp_ms < query.from_ms || s.timestamp_ms > query.to_ms {
                return false;
            }
            true
        }).cloned().collect();

        Ok(filtered)
    }
}
