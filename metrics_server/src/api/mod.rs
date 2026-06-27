use crate::storage::SharedStore;
use crate::telemetry::registry::TelemetryRegistry;
use crate::model::AppConfig;

#[derive(Clone)]
pub struct ApiState {
    pub config: AppConfig,
    pub store: SharedStore,
    pub telemetry: TelemetryRegistry,
}

pub mod routes;
pub mod handlers;
