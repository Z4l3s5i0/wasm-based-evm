use axum::{
    routing::{get, post},
    Router,
    response::Html,
    Form,
};
use maud::{html, DOCTYPE};
use crate::logging::LOGS;
use std::net::SocketAddr;
use crate::{info, error};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct RpcForm {
    method: String,
    params: String,
}

pub async fn start_frontend(addr: SocketAddr, rpc_port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rpc_url = format!("http://127.0.0.1:{}", rpc_port);
    let app = Router::new()
        .route("/", get(index))
        .route("/logs", get(get_logs))
        .route("/rpc", get(rpc_page))
        .route("/rpc/execute", post(move |form| execute_rpc(form, rpc_url.clone())));

    info!("[Frontend] Serving at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn layout(title: &str, content: maud::Markup) -> maud::Markup {
    html! {
        (DOCTYPE)
        html {
            head {
                title { (title) }
                script src="https://unpkg.com/htmx.org@1.9.10" {}
                style {
                    "body { font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; background-color: #121212; color: #e0e0e0; margin: 0; display: flex; flex-direction: column; height: 100vh; }"
                    "nav { background-color: #1f1f1f; padding: 1rem; border-bottom: 1px solid #333; display: flex; gap: 20px; }"
                    "nav a { color: #00ff00; text-decoration: none; font-weight: bold; }"
                    "nav a:hover { text-decoration: underline; }"
                    "main { padding: 20px; flex: 1; overflow: hidden; display: flex; flex-direction: column; }"
                    ".log-container { background-color: #000; border: 1px solid #333; padding: 10px; flex: 1; overflow-y: scroll; display: flex; flex-direction: column-reverse; font-family: monospace; font-size: 0.9rem; }"
                    ".log-entry { margin-bottom: 4px; border-bottom: 1px solid #1a1a1a; padding-bottom: 2px; white-space: pre-wrap; word-break: break-all; }"
                    ".INFO { color: #00ff00; }"
                    ".DEBUG { color: #0088ff; }"
                    ".ERROR { color: #ff3333; }"
                    "form { background-color: #1f1f1f; padding: 20px; border-radius: 8px; border: 1px solid #333; max-width: 800px; }"
                    ".field { margin-bottom: 15px; }"
                    "label { display: block; margin-bottom: 5px; font-weight: bold; }"
                    "input, textarea { width: 100%; padding: 8px; background: #2c2c2c; border: 1px solid #444; color: #fff; border-radius: 4px; box-sizing: border-box; }"
                    "button { background-color: #008cba; color: white; padding: 10px 20px; border: none; border-radius: 4px; cursor: pointer; font-size: 1rem; }"
                    "button:hover { background-color: #007ba1; }"
                    "#rpc-result { margin-top: 20px; padding: 15px; background: #000; border: 1px solid #444; border-radius: 4px; font-family: monospace; white-space: pre-wrap; overflow-x: auto; }"
                }
            }
            body {
                nav {
                    a href="/" { "Logs" }
                    a href="/rpc" { "RPC Console" }
                }
                main {
                    (content)
                }
            }
        }
    }
}

async fn index() -> Html<String> {
    let content = html! {
        h1 { "Node Logs" }
        div class="log-container" hx-get="/logs" hx-trigger="every 2s" {
            (get_logs_markup())
        }
    };
    Html(layout("Wasix EVM - Logs", content).into_string())
}

async fn rpc_page() -> Html<String> {
    let content = html! {
        h1 { "RPC Console" }
        form hx-post="/rpc/execute" hx-target="#rpc-result" {
            div class="field" {
                label for="method" { "Method" }
                input type="text" id="method" name="method" placeholder="eth_blockNumber" required;
            }
            div class="field" {
                label for="params" { "Params (JSON Array)" }
                textarea id="params" name="params" rows="5" placeholder="[]" { "[]" }
            }
            button type="submit" { "Execute" }
        }
        div id="rpc-result" { "Result will appear here..." }
    };
    Html(layout("Wasix EVM - RPC Console", content).into_string())
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

async fn execute_rpc(Form(form): Form<RpcForm>, rpc_url: String) -> Html<String> {
    let client = reqwest::Client::new();
    let params_val: serde_json::Value = match serde_json::from_str(&form.params) {
        Ok(v) => v,
        Err(e) => return Html(format!("Invalid JSON params: {}", e)),
    };

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": form.method,
        "params": params_val
    });

    match client.post(&rpc_url).json(&body).send().await {
        Ok(resp) => {
            match resp.text().await {
                Ok(text) => {
                    // Try to pretty print if it's JSON
                    let display_text = match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(json) => serde_json::to_string_pretty(&json).unwrap_or(text),
                        Err(_) => text,
                    };
                    Html(display_text)
                }
                Err(e) => Html(format!("Error reading response: {}", e)),
            }
        }
        Err(e) => {
            error!("[Frontend] RPC execution failed: {}", e);
            Html(format!("RPC Error: {}", e))
        }
    }
}
