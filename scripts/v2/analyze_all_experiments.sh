#!/bin/bash
set -e

# Analyze All Experiments Script
# Usage: ./analyze_all_experiments.sh <experiments_base_dir> [chain_id]
# Iterates over every experiment directory in the provided path and runs analyze_experiment.sh

BASE_DIR=$1
CHAIN_ID=${2:-""}

if [ -z "$BASE_DIR" ]; then
    echo "Usage: $0 <experiments_base_dir> [chain_id]"
    exit 1
fi

if [ ! -d "$BASE_DIR" ]; then
    echo "Error: Directory $BASE_DIR does not exist."
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ANALYZE_SCRIPT="$SCRIPT_DIR/analyze_experiment.sh"

if [ ! -f "$ANALYZE_SCRIPT" ]; then
    echo "Error: $ANALYZE_SCRIPT not found."
    exit 1
fi

echo "=== Starting Bulk Analysis in $BASE_DIR ==="

# Get absolute path of BASE_DIR to avoid issues with relative paths in the loop
ABS_BASE_DIR=$(cd "$BASE_DIR" && pwd)

# Count total directories for progress tracking
TOTAL_DIRS=$(find "$ABS_BASE_DIR" -maxdepth 1 -mindepth 1 -type d | wc -l)
COUNTER=0

echo "Found $TOTAL_DIRS directories to process."
echo ""

# Iterate over each directory in the base path
for exp_dir in "$ABS_BASE_DIR"/*; do
    if [ -d "$exp_dir" ]; then
        COUNTER=$((COUNTER + 1))
        exp_name=$(basename "$exp_dir")
        
        echo "--- Processing ($COUNTER/$TOTAL_DIRS): $exp_name ---"
        
        # Check if it looks like an experiment directory
        # (e.g., contains data_v2, metrics_server_data, or logs_dump)
        if [ -d "$exp_dir/data_v2" ] || [ -d "$exp_dir/metrics_server_data" ] || [ -d "$exp_dir/logs_dump" ] || [ -d "$exp_dir/aggregated_logs" ]; then
            bash "$ANALYZE_SCRIPT" "$exp_dir" "$CHAIN_ID"
        else
            echo "Skipping $exp_name: Not a recognized experiment directory."
        fi
        
        echo "---------------------------------------------------"
        echo ""
    fi
done

echo "=== Bulk Analysis Complete ==="
