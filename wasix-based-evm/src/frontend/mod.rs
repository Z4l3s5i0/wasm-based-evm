pub mod rpc_page;
pub mod logs_page;

use axum::{
    routing::{get, post},
    Router,
    response::Html,
    Form,
};
use maud::{html, DOCTYPE};
use std::net::SocketAddr;
use crate::{info};
use serde::{Deserialize, Deserializer, de};
use std::fmt;
use crate::frontend::logs_page::{get_logs_markup, index};
use crate::frontend::rpc_page::{execute_rpc, rpc_page};

#[derive(Deserialize)]
pub struct RpcForm {
    pub method: String,
    #[serde(deserialize_with = "deserialize_params")]
    pub params: Vec<String>,
}

fn deserialize_params<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ParamsVisitor;

    impl<'de> de::Visitor<'de> for ParamsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or a sequence of strings")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![v.to_string()])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(ParamsVisitor)
}

#[derive(Deserialize)]
pub struct MethodQuery {
    pub method: String,
}

pub async fn start_frontend(addr: SocketAddr, rpc_port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rpc_url = format!("http://127.0.0.1:{}", rpc_port);
    let app = Router::new()
        .route("/", get(index))
        .route("/logs", get(get_logs))
        .route("/rpc", get(rpc_page))
        .route("/rpc/params", get(crate::frontend::rpc_page::rpc_params))
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

async fn get_logs() -> Html<String> {
    Html(get_logs_markup().into_string())
}

