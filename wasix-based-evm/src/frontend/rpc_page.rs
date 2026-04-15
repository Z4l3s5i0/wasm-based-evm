use axum::{
    response::Html,
    Form,
    extract::Query,
};
use maud::{html};
use crate::{error};
use crate::frontend::{layout, RpcForm, MethodQuery};

pub async fn rpc_page() -> Html<String> {
    let methods = vec![
        "eth_blockNumber",
        "eth_chainId",
        "eth_getBalance",
        "eth_getTransactionCount",
        "eth_getCode",
        "eth_getStorageAt",
        "eth_getBlockByNumber",
        "eth_getBlockByHash",
        "eth_gasPrice",
        "eth_accounts",
        "eth_syncing",
        "eth_mining",
        "eth_sendTransaction",
        "eth_call",
        "eth_estimateGas",
    ];

    let content = html! {
        h1 { "RPC Console" }
        form hx-post="/rpc/execute" hx-target="#rpc-result" {
            div class="field" {
                label for="method" { "Method" }
                select id="method" name="method" hx-get="/rpc/params" hx-target="#params-container" {
                    option value="" selected { "Select a method..." }
                    @for method in methods {
                        option value=(method) { (method) }
                    }
                }
            }
            div id="params-container" {
                (maud::PreEscaped(rpc_params(Query(MethodQuery { method: "".to_string() })).await.0))
            }
            button type="submit" { "Execute" }
        }
        div id="rpc-result" { "Result will appear here..." }
    };
    Html(layout("Wasix EVM - RPC Console", content).into_string())
}

pub async fn rpc_params(Query(query): Query<MethodQuery>) -> Html<String> {
    let content = match query.method.as_str() {
        "eth_getBalance" | "eth_getTransactionCount" | "eth_getCode" => html! {
            div class="field" {
                label for="params" { "Address" }
                input type="text" name="params" placeholder="0x..." required;
            }
        },
        "eth_getStorageAt" => html! {
            div class="field" {
                label for="address" { "Address" }
                input type="text" name="params" placeholder="0x..." required;
            }
            div class="field" {
                label for="slot" { "Slot" }
                input type="text" name="params" placeholder="0x..." required;
            }
        },
        "eth_getBlockByNumber" => html! {
            div class="field" {
                label for="block" { "Block (latest, pending, or hex)" }
                input type="text" name="params" placeholder="latest" required;
            }
            div class="field" {
                label {
                    input type="checkbox" name="params" value="true" style="width: auto; margin-right: 10px;";
                    "Full transaction objects"
                }
            }
            // Hidden input to ensure we always send a second param even if checkbox is unchecked
            input type="hidden" name="params" value="false";
        },
        "eth_getBlockByHash" => html! {
            div class="field" {
                label for="hash" { "Block Hash" }
                input type="text" name="params" placeholder="0x..." required;
            }
            div class="field" {
                label {
                    input type="checkbox" name="params" value="true" style="width: auto; margin-right: 10px;";
                    "Full transaction objects"
                }
            }
            input type="hidden" name="params" value="false";
        },
        "eth_sendTransaction" | "eth_call" | "eth_estimate_gas" => html! {
             div class="field" {
                label for="params" { "Transaction Request (JSON)" }
                textarea name="params" rows="5" placeholder="{ \"from\": \"0x...\", \"to\": \"0x...\", \"data\": \"0x...\" }" required;
            }
        },
        _ if query.method.is_empty() => html! {
            div class="field" {
                label for="params" { "Params (JSON Array)" }
                textarea id="params" name="params" rows="5" placeholder="[]" { "[]" }
            }
        },
        _ => html! {
            div class="field" {
                label for="params" { "Params (JSON Array)" }
                textarea id="params" name="params" rows="5" placeholder="[]" { "[]" }
            }
        }
    };
    Html(content.into_string())
}

pub async fn execute_rpc(Form(form): Form<RpcForm>, rpc_url: String) -> Html<String> {
    let client = reqwest::Client::new();
    
    // Convert Vec<String> to JSON params
    let params_val = if form.params.len() == 1 && form.params[0].trim().starts_with('[') {
        // If it looks like a JSON array in a single field, parse it
        match serde_json::from_str::<serde_json::Value>(&form.params[0]) {
            Ok(v) => v,
            Err(e) => return Html(format!("Invalid JSON params: {}", e)),
        }
    } else {
        // Otherwise, treat each entry in the vector as a separate parameter
        let mut vals = Vec::new();
        for p in form.params {
            if p == "true" {
                vals.push(serde_json::Value::Bool(true));
            } else if p == "false" {
                vals.push(serde_json::Value::Bool(false));
            } else if let Ok(n) = p.parse::<i64>() {
                vals.push(serde_json::Value::Number(n.into()));
            } else if p.trim().starts_with('{') {
                // Try to parse as JSON object for transaction requests
                match serde_json::from_str::<serde_json::Value>(&p) {
                    Ok(v) => vals.push(v),
                    Err(_) => vals.push(serde_json::Value::String(p)),
                }
            } else if !p.is_empty() {
                vals.push(serde_json::Value::String(p));
            }
        }
        serde_json::Value::Array(vals)
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
