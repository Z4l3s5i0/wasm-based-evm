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
FAILED_PLOTS=()

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
                    
                    # Check if plotting was skipped
                    # We need to find the experiment directory. run_experiment.sh doesn't output it easily,
                    # but we know it's in experiments/exp_YYYYMMDD_HHMMSS/analysis/PLOTS_SKIPPED
                    # Let's find the latest directory in experiments/
                    LATEST_EXP=$(ls -td experiments/exp_* 2>/dev/null | head -1)
                    if [ -f "$LATEST_EXP/analysis/PLOTS_SKIPPED" ]; then
                        PARAM_INFO="Nodes: $nodes, Ratio: $ratio, WL%: $wl_percent, Dur: $duration, Wait: $wait_dur"
                        FAILED_PLOTS+=("$LATEST_EXP ($PARAM_INFO)")
                    fi
                    
                    echo "--- Finished Experiment $COUNTER/$TOTAL_COMBINATIONS ---"
                    echo ""
                done
            done
        done
    done
done

echo "=== All Experiments Completed ==="

if [ ${#FAILED_PLOTS[@]} -ne 0 ]; then
    echo ""
    echo "=== Experiments with skipped plotting (invalid/empty metrics) ==="
    for failed in "${FAILED_PLOTS[@]}"; do
        echo "- $failed"
    done
    echo "================================================================"
fi
