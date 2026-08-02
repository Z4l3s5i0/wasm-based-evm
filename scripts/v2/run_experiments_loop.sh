#!/bin/bash
set -e

# Experiment Loop Script
# Runs the experiment with different combinations of parameters.

DURATIONS=(30 60 90 120 150)
WAIT_DURATIONS=(480)
NODE_COUNTS=(5 10 15)
RATIOS=(100 75 50 25 0)
WORKLOAD_NODE_PERCENTS=(100 50)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUN_EXPERIMENT="$SCRIPT_DIR/run_experiment.sh"

TOTAL_COMBINATIONS=$((${#DURATIONS[@]} * ${#WAIT_DURATIONS[@]} * ${#NODE_COUNTS[@]} * ${#RATIOS[@]} * ${#WORKLOAD_NODE_PERCENTS[@]}))
COUNTER=0

echo "=== Starting Experiment Loop ==="
echo "Total combinations to run: $TOTAL_COMBINATIONS"
echo ""

for nodes in "${NODE_COUNTS[@]}"; do
    for ratio in "${RATIOS[@]}"; do
        for wl_percent in "${WORKLOAD_NODE_PERCENTS[@]}"; do
            for duration in "${DURATIONS[@]}"; do
                for wait_dur in "${WAIT_DURATIONS[@]}"; do
                    COUNTER=$((COUNTER + 1))
                    
                    echo "--- Running Experiment $COUNTER/$TOTAL_COMBINATIONS ---"
                    echo "Nodes: $nodes"
                    echo "Ratio (Linux %): $ratio"
                    echo "Workload Percent: $wl_percent%"
                    echo "Workload Duration: $duration s"
                    echo "Wait Duration: $wait_dur s"
                    echo "---------------------------------------"
                    
                    # Run the experiment
                    # We use a unique chain-id for each run to avoid any state overlap if cleanup fails
                    CHAIN_ID=$((1000 + COUNTER))
                    
                    bash "$RUN_EXPERIMENT" \
                        --nodes "$nodes" \
                        --ratio "$ratio" \
                        --workload-percent "$wl_percent" \
                        --duration "$duration" \
                        --wait-duration "$wait_dur" \
                        --chain-id "$CHAIN_ID"
                    
                    echo "--- Finished Experiment $COUNTER/$TOTAL_COMBINATIONS ---"
                    echo ""
                done
            done
        done
    done
done

echo "=== All Experiments Completed ==="
