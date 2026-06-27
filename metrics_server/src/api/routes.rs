use axum::{routing::get, Router};
use crate::api::{ApiState, handlers};

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(handlers::health_handler))
        .route("/metrics", get(handlers::metrics_handler))
        .route("/api/nodes", get(handlers::nodes_handler))
        .route("/api/experiments", get(handlers::experiments_handler))
        .route("/api/metrics/latest", get(handlers::latest_metrics_handler))
        .route("/api/metrics/range", get(handlers::range_metrics_handler))
        .with_state(state)
}
