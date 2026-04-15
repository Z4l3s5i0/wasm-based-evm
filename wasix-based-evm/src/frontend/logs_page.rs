use axum::response::Html;
use maud::html;
use crate::frontend::layout;
use crate::logging::LOGS;

pub async fn index() -> Html<String> {
    let content = html! {
        h1 { "Node Logs" }
        div class="log-container" hx-get="/logs" hx-trigger="every 2s" {
            (get_logs_markup())
        }
    };
    Html(layout("Wasix EVM - Logs", content).into_string())
}

pub fn get_logs_markup() -> maud::Markup {
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
