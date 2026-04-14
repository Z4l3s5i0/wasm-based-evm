use axum::{
    routing::get,
    Router,
    response::Html,
};
use maud::{html, DOCTYPE};
use crate::logging::LOGS;
use std::net::SocketAddr;
use crate::info;

pub async fn start_frontend(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/", get(index))
        .route("/logs", get(get_logs));

    info!("[Frontend] Serving at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn index() -> Html<String> {
    let markup = html! {
        (DOCTYPE)
        html {
            head {
                title { "Wasix EVM Logs" }
                script src="https://unpkg.com/htmx.org@1.9.10" {}
                style {
                    "body { font-family: monospace; background-color: #1a1a1a; color: #00ff00; padding: 20px; }"
                    ".log-container { border: 1px solid #333; padding: 10px; height: 80vh; overflow-y: scroll; display: flex; flex-direction: column-reverse; }"
                    ".log-entry { margin-bottom: 5px; border-bottom: 1px solid #222; padding-bottom: 2px; }"
                    ".INFO { color: #00ff00; }"
                    ".DEBUG { color: #0088ff; }"
                    ".ERROR { color: #ff0000; }"
                }
            }
            body {
                h1 { "Wasix EVM Node Logs" }
                div class="log-container" hx-get="/logs" hx-trigger="every 1s" {
                    (get_logs_markup())
                }
            }
        }
    };
    Html(markup.into_string())
}

async fn get_logs() -> Html<String> {
    Html(get_logs_markup().into_string())
}

fn get_logs_markup() -> maud::Markup {
    let logs = LOGS.lock().unwrap();
    html! {
        @for log in logs.iter().rev() {
            @let level = if log.contains("[INFO]") { "INFO" } else if log.contains("[DEBUG]") { "DEBUG" } else if log.contains("[ERROR]") { "ERROR" } else { "" };
            div class=(format!("log-entry {}", level)) {
                (log)
            }
        }
    }
}
