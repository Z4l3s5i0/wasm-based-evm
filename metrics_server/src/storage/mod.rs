use anyhow::Result;
use std::sync::Arc;
use crate::model::{MetricSample, MetricRangeQuery, Node, Experiment, RpcObservation, CollectionError};

pub trait MetricsStore: Send + Sync {
    fn upsert_experiment(&self, experiment: &Experiment) -> Result<()>;
    fn upsert_node(&self, node: &Node) -> Result<()>;

    fn insert_rpc_observation(&self, observation: &RpcObservation) -> Result<()>;
    fn insert_metric_sample(&self, sample: &MetricSample) -> Result<()>;
    fn insert_collection_error(&self, error: &CollectionError) -> Result<()>;

    fn list_nodes(&self) -> Result<Vec<Node>>;
    fn list_experiments(&self) -> Result<Vec<Experiment>>;

    fn latest_metrics(&self) -> Result<Vec<MetricSample>>;
    fn query_metric_range(&self, query: &MetricRangeQuery) -> Result<Vec<MetricSample>>;
}

pub type SharedStore = Arc<dyn MetricsStore>;

pub mod memory;
pub mod redb_store;
