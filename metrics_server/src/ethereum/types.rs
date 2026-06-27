use anyhow::{Result, anyhow};
use serde_json::Value;

pub fn parse_hex_u64(value: &str) -> Result<u64> {
    let clean = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(clean, 16).map_err(|e| anyhow!("failed to parse hex u64: {}", e))
}

pub fn parse_hex_f64(value: &str) -> Result<f64> {
    let clean = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(clean, 16).map(|v| v as f64).map_err(|e| anyhow!("failed to parse hex f64: {}", e))
}

pub fn syncing_to_numeric(value: &Value) -> f64 {
    match value {
        Value::Bool(b) => if *b { 1.0 } else { 0.0 },
        Value::Object(_) => 1.0,
        _ => 0.0,
    }
}
