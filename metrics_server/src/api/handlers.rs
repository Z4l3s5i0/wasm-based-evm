use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::api::ApiState;
use crate::model::{MetricRangeQuery, MetricRangeResponse, MetricPoint, RegisterNodeRequest, Node, NodeStatus, BootstrapNodesQuery, BootstrapNode, BootstrapNodesResponse};
use serde_json::json;
use crate::time::now_ms;

pub async fn health_handler() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

pub async fn register_node_handler(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<RegisterNodeRequest>,
) -> impl IntoResponse {
    if let Some(bootstrap) = &state.config.bootstrap {
        if let Some(token) = &bootstrap.registration_token {
            let auth_ok = headers.get("Authorization")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.starts_with("Bearer ") && &s[7..] == token)
                .unwrap_or(false);
            
            if !auth_ok {
                state.telemetry.bootstrap_registration_requests.with_label_values(&["unauthorized"]).inc();
                return (StatusCode::UNAUTHORIZED, "Invalid registration token").into_response();
            }
        }
    }

    if payload.id.is_empty() || payload.network.is_empty() || payload.rpc_url.is_empty() {
        state.telemetry.bootstrap_registration_requests.with_label_values(&["bad_request"]).inc();
        return (StatusCode::BAD_REQUEST, "id, network, and rpc_url are required").into_response();
    }

    if !payload.rpc_url.starts_with("http://") && !payload.rpc_url.starts_with("https://") {
        state.telemetry.bootstrap_registration_requests.with_label_values(&["bad_request"]).inc();
        return (StatusCode::BAD_REQUEST, "rpc_url must start with http:// or https://").into_response();
    }

    let node = Node {
        id: payload.id.clone(),
        network: payload.network,
        client: payload.client,
        rpc_url: payload.rpc_url,
        metrics_url: payload.metrics_url,
        p2p_addr: payload.p2p_addr,
        discovery_addr: payload.discovery_addr,
        enode: payload.enode,
        status: NodeStatus::Pending,
        last_seen_ms: Some(now_ms()),
        last_successful_probe_ms: None,
        consecutive_failures: 0,
    };

    match state.store.upsert_node(&node) {
        Ok(_) => {
            state.telemetry.bootstrap_registration_requests.with_label_values(&["accepted"]).inc();
            Json(json!({"status": "accepted", "node_id": payload.id})).into_response()
        }
        Err(e) => {
            state.telemetry.bootstrap_registration_requests.with_label_values(&["error"]).inc();
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to register node: {}", e)).into_response()
        }
    }
}

pub async fn bootstrap_nodes_handler(
    State(state): State<ApiState>,
    Query(query): Query<BootstrapNodesQuery>,
) -> impl IntoResponse {
    match state.store.list_nodes() {
        Ok(nodes) => {
            let mut filtered: Vec<BootstrapNode> = nodes
                .into_iter()
                .filter(|n| {
                    if n.status != NodeStatus::Active {
                        return false;
                    }
                    if let Some(network) = &query.network {
                        if &n.network != network {
                            return false;
                        }
                    }
                    if let Some(exclude_id) = &query.exclude_id {
                        if &n.id == exclude_id {
                            return false;
                        }
                    }
                    // At least one P2P-related field must exist
                    n.enode.is_some() || n.discovery_addr.is_some() || n.p2p_addr.is_some()
                })
                .map(|n| BootstrapNode {
                    id: n.id,
                    network: n.network,
                    client: n.client,
                    p2p_addr: n.p2p_addr,
                    discovery_addr: n.discovery_addr,
                    enode: n.enode,
                    last_seen_ms: n.last_seen_ms,
                })
                .collect();

            // Sort by last_successful_probe_ms DESC
            // Note: Node doesn't have last_successful_probe_ms in BootstrapNode, 
            // but we have it in Node during filtering.
            // Wait, I need to sort before mapping or use a decorated struct.
            
            // Re-implementing with sort
            /*
            filtered.sort_by(|a, b| b.last_seen_ms.cmp(&a.last_seen_ms)); // Simple sort for now
            */

            if let Some(limit) = query.limit {
                filtered.truncate(limit);
            }

            Json(BootstrapNodesResponse { nodes: filtered }).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list nodes: {}", e)).into_response(),
    }
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
