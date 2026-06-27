use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::api::ApiState;
use crate::model::{MetricRangeQuery, MetricRangeResponse, MetricPoint};
use serde_json::json;

pub async fn health_handler() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

pub async fn metrics_handler(State(state): State<ApiState>) -> impl IntoResponse {
    match state.telemetry.gather_text() {
        Ok(text) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; version=0.0.4")],
            text,
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to gather metrics: {}", e),
        ).into_response(),
    }
}

pub async fn nodes_handler(State(state): State<ApiState>) -> impl IntoResponse {
    match state.store.list_nodes() {
        Ok(nodes) => Json(nodes).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list nodes: {}", e),
        ).into_response(),
    }
}

pub async fn experiments_handler(State(state): State<ApiState>) -> impl IntoResponse {
    match state.store.list_experiments() {
        Ok(exps) => Json(exps).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list experiments: {}", e)).into_response(),
    }
}

pub async fn latest_metrics_handler(State(state): State<ApiState>) -> impl IntoResponse {
    match state.store.latest_metrics() {
        Ok(metrics) => Json(metrics).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get latest metrics: {}", e)).into_response(),
    }
}

pub async fn range_metrics_handler(
    State(state): State<ApiState>,
    Query(query): Query<MetricRangeQuery>,
) -> impl IntoResponse {
    if query.from_ms > query.to_ms {
        return (StatusCode::BAD_REQUEST, "from_ms must be <= to_ms").into_response();
    }

    match state.store.query_metric_range(&query) {
        Ok(samples) => {
            let points = samples
                .into_iter()
                .map(|s| MetricPoint {
                    timestamp_ms: s.timestamp_ms,
                    value: s.value,
                })
                .collect();
            
            Json(MetricRangeResponse {
                metric: query.metric,
                node_id: query.node_id,
                experiment_id: query.experiment_id,
                points,
            }).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to query metric range: {}", e)).into_response(),
    }
}
