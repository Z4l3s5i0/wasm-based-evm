use crate::config;
use crate::storage::{memory::MemoryStore, redb_store::RedbStore, SharedStore};
use crate::telemetry::registry::TelemetryRegistry;
use crate::api::{ApiState, routes};
use crate::collector::scheduler::CollectorScheduler;
use crate::model::{Experiment, Node};
use crate::time::now_ms;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;
use tracing::{info, error};

pub async fn run(config_path: PathBuf) -> anyhow::Result<()> {
    let config = config::load_config(&config_path)?;
    config::validate_config(&config)?;

    tracing_subscriber::fmt::init();
    info!("Starting metrics_server with config: {:?}", config_path);

    let store: SharedStore = match config.storage.kind.as_str() {
        "memory" => Arc::new(MemoryStore::new()),
        "redb" => Arc::new(RedbStore::new(&config.storage.path)?),
        _ => unreachable!(),
    };

    if let Some(exp_config) = &config.experiment {
        let experiment = Experiment {
            id: exp_config.id.clone(),
            name: exp_config.name.clone(),
            description: exp_config.description.clone(),
            tags: exp_config.tags.clone(),
            started_at_ms: now_ms(),
            ended_at_ms: None,
        };
        store.upsert_experiment(&experiment)?;
    }

    for node_config in &config.nodes {
        let node = Node {
            id: node_config.id.clone(),
            network: node_config.network.clone(),
            client: node_config.client.clone(),
            rpc_url: node_config.rpc_url.clone(),
            metrics_url: node_config.metrics_url.clone(),
            p2p_addr: None,
            discovery_addr: None,
            enode: None,
            status: crate::model::NodeStatus::Active,
            last_seen_ms: Some(now_ms()),
            last_successful_probe_ms: None,
            consecutive_failures: 0,
        };
        store.upsert_node(&node)?;
    }

    let telemetry = TelemetryRegistry::new()?;

    let state = ApiState {
        config: config.clone(),
        store: store.clone(),
        telemetry: telemetry.clone(),
    };

    let app = routes::router(state);
    let addr: std::net::SocketAddr = config.server.bind_addr.parse()?;
    
    info!("HTTP API listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let prober = crate::bootstrap::prober::BootstrapProber::new(config.clone(), store.clone(), telemetry.clone());
    let prober_rx = shutdown_rx.clone();
    let prober_handle = tokio::spawn(async move {
        if let Err(e) = prober.run(prober_rx).await {
            error!("Bootstrap prober failed: {}", e);
        }
    });

    let scheduler = CollectorScheduler::new(config, store, telemetry);
    let scheduler_handle = tokio::spawn(async move {
        if let Err(e) = scheduler.run(shutdown_rx).await {
            error!("Collector scheduler failed: {}", e);
        }
    });

    let _server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("HTTP server failed: {}", e);
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down");
            let _ = shutdown_tx.send(true);
        }
    }

    // Wait for tasks to finish
    let _ = scheduler_handle.await;
    let _ = prober_handle.await;
    // Axum serve doesn't have a simple graceful shutdown in this version without more boilerplate,
    // but the process will exit anyway.

    Ok(())
}
