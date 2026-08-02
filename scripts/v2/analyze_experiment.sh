#!/bin/bash
set -e

# Experiment Analysis Script
# Usage: ./analyze_experiment.sh <experiment_dir> [chain_id]

EXP_DIR=$1
CHAIN_ID=${2:-""}

if [ -z "$EXP_DIR" ]; then
    echo "Usage: $0 <experiment_dir> [chain_id]"
    exit 1
fi

if [ ! -d "$EXP_DIR" ]; then
    echo "Error: Directory $EXP_DIR does not exist."
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DB_EXPORTER="$SCRIPT_DIR/db_exporter/target/release/db_exporter"

# 0. Ensure db_exporter is built
if [ ! -f "$DB_EXPORTER" ]; then
    echo "Building db_exporter..."
    (cd "$SCRIPT_DIR/db_exporter" && cargo build --release)
fi

echo "Starting analysis for experiment: $EXP_DIR"
mkdir -p "$EXP_DIR/analysis"

# 1. Extract Metrics Server Data
METRICS_DB="$EXP_DIR/metrics_server_data/metrics.redb"
if [ -f "$METRICS_DB" ]; then
    echo "Extracting Metrics Server data..."
    METRICS_TABLES=("experiments" "nodes" "metric_samples" "rpc_observations")
    for table in "${METRICS_TABLES[@]}"; do
        echo "  Exporting table: $table"
        "$DB_EXPORTER" export --db-path "$METRICS_DB" --table "$table" --db-type metrics > "$EXP_DIR/analysis/metrics_$table.json" 2>/dev/null || echo "    Warning: Could not export $table"
    done
else
    echo "Warning: Metrics database not found at $METRICS_DB"
fi

# 2. Extract EL Node Data
echo "Extracting EL node data..."
EL_TABLES=("headers" "block_bodies" "transactions" "metadata")
# Find all node directories in data_v2
if [ -d "$EXP_DIR/data_v2" ]; then
    for node_dir in "$EXP_DIR/data_v2"/node-*; do
        [ -d "$node_dir" ] || continue
        node_name=$(basename "$node_dir")
        # Path to EL database might vary, check common locations
        EL_DB=""
        if [ -f "$node_dir/el/linux-$node_name" ]; then
             EL_DB="$node_dir/el/linux-$node_name"
        elif [ -f "$node_dir/el/wasix-$node_name" ]; then
             EL_DB="$node_dir/el/wasix-$node_name"
        elif [ -f "$node_dir/el/eth.redb" ]; then
             EL_DB="$node_dir/el/eth.redb"
        fi
        
        if [ -n "$EL_DB" ]; then
            echo "  Checking EL data for $node_name at $EL_DB..."
            mkdir -p "$EXP_DIR/analysis/$node_name"
            # List available tables first to debug
            "$DB_EXPORTER" list --db-path "$EL_DB" > "$EXP_DIR/analysis/$node_name/tables.txt" 2>/dev/null
            
            for table in "${EL_TABLES[@]}"; do
                "$DB_EXPORTER" export --db-path "$EL_DB" --table "$table" --db-type el > "$EXP_DIR/analysis/$node_name/el_$table.json" 2>/dev/null || echo "    Warning: Could not export $table for $node_name"
            done
        else
            echo "  Warning: No EL database found for $node_name"
        fi
    done
else
    echo "Warning: data_v2 directory not found."
fi

# 3. Evaluate Logs
echo "Evaluating logs..."
LOGS_DIR="$EXP_DIR/logs_dump"
if [ ! -d "$LOGS_DIR" ]; then
    # Try aggregated logs if logs_dump is missing
    LOGS_DIR="$EXP_DIR/aggregated_logs"
fi

if [ -d "$LOGS_DIR" ]; then
    bash "$SCRIPT_DIR/evaluate.sh" "$LOGS_DIR" "$CHAIN_ID" > "$EXP_DIR/analysis/log_evaluation.txt"
    echo "  Log evaluation saved to $EXP_DIR/analysis/log_evaluation.txt"
else
    echo "Warning: No logs found for evaluation."
fi

# 4. Generate Plots
echo "Generating plots..."
python "$SCRIPT_DIR/plot_metrics.py" "$EXP_DIR/analysis" "$EXP_DIR/plots"

# 5. Generate Summary
echo "Analysis complete. Results are in $EXP_DIR/analysis and $EXP_DIR/plots"

cat <<EOF > "$EXP_DIR/analysis/SUMMARY.md"
# Experiment Analysis Summary
Experiment: $(basename "$EXP_DIR")
Date: $(date)

## Metrics
- Metrics Server data exported to JSON files.
- EL node data (Headers, BlockBodies, Transactions) exported per node.
- Visualizations generated in [plots folder](../plots).

## Log Evaluation
See [log_evaluation.txt](./log_evaluation.txt) for detailed log analysis results.
EOF
