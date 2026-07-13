use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use redb::{Database, TableDefinition, ReadableDatabase, ReadableTable};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Export metrics-server redb to JSON
    Metrics {
        /// Path to the metrics.redb file
        #[arg(short, long)]
        db_path: String,
        /// Table to export (nodes, experiments, metric_samples, rpc_observations)
        #[arg(short, long)]
        table: String,
    },
    /// List tables in a redb file
    List {
        /// Path to the redb file
        #[arg(short, long)]
        db_path: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Metrics { db_path, table } => {
            export_metrics(&db_path, &table)?;
        }
        Commands::List { db_path } => {
            list_tables(&db_path)?;
        }
    }

    Ok(())
}

fn list_tables(db_path: &str) -> Result<()> {
    let db = Database::open(db_path).context("Failed to open database")?;
    let read_txn = db.begin_read()?;
    
    println!("Tables in {}:", db_path);
    // redb doesn't easily expose list of table names without a write txn or knowing them
    // but we can try some common ones
    let common_tables = vec![
        "nodes", "experiments", "metric_samples", "rpc_observations", 
        "collection_errors", "latest_metrics", "counters",
        "headers", "transactions", "accounts", "metadata"
    ];

    for table_name in common_tables {
        let definition: TableDefinition<&str, &str> = TableDefinition::new(table_name);
        if read_txn.open_table(definition).is_ok() {
            println!("  - {}", table_name);
        }
        
        let definition_u64: TableDefinition<&str, u64> = TableDefinition::new(table_name);
        if read_txn.open_table(definition_u64).is_ok() {
            println!("  - {} (u64 values)", table_name);
        }
    }

    Ok(())
}

fn export_metrics(db_path: &str, table_name: &str) -> Result<()> {
    let db = Database::open(db_path).context("Failed to open database")?;
    let read_txn = db.begin_read()?;
    
    let definition: TableDefinition<&str, &str> = TableDefinition::new(table_name);
    let table = read_txn.open_table(definition).context(format!("Table {} not found", table_name))?;
    
    let mut results = Vec::new();
    for item in table.iter()? {
        let (key, value) = item?;
        let key_str = key.value();
        let val_str = value.value();
        
        // Try to parse value as JSON
        let json_val: Value = serde_json::from_str(val_str).unwrap_or(Value::String(val_str.to_string()));
        
        results.push(json!({
            "key": key_str,
            "value": json_val
        }));
    }

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
