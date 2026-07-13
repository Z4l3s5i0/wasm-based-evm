#!/bin/bash
set -e

# Experiment Automation Script
# Orchestrates the full lifecycle of an experiment run.

CHAIN_ID=10
WORKLOAD_DURATION=300 # Default workload duration in seconds
NODES=10
RATIO=0

usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  --duration N     Duration to run the workload in seconds (default: $WORKLOAD_DURATION)"
    echo "  --chain-id ID    Chain ID to use (default: $CHAIN_ID)"
    echo "  --nodes N        Number of nodes to start (default: $NODES)"
    echo "  --ratio N        Percent of rustcompiled nodes (default: $RATIO)"
    exit 1
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --duration) WORKLOAD_DURATION="$2"; shift ;;
        --chain-id) CHAIN_ID="$2"; shift ;;
        --nodes) NODES="$2"; shift ;;
        --ratio) RATIO="$2"; shift ;;
        *) usage ;;
    esac
    shift
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RATIO2=100-$RATIO

echo "=== Starting Experiment Workflow ==="
echo "Chain ID: $CHAIN_ID"
echo "Workload Duration: $WORKLOAD_DURATION seconds"
echo "Nodes: $NODES"
echo "RATIO: $RATIO Rust-compiled, $RATIO2 WASM-compiled"

# 1. Start the local network
echo "Step 1: Starting local network..."
bash "$SCRIPT_DIR/start_local.sh" --nodes "$NODES" --chain-id "$CHAIN_ID" --linux "$RATIO" --wasix "$RATIO2" --setup --cleanup

# 2. Start manage_logs
echo "Step 2: Starting background log processor..."
bash "$SCRIPT_DIR/manage_logs.sh" start --chain-id "$CHAIN_ID"

# 3. Wait for 180 seconds
echo "Step 3: Waiting 180 seconds for network stabilization..."
sleep 180

# 4. Start run_workload in background
echo "Step 4: Starting workload in background..."
# We pass --duration slightly longer than WORKLOAD_DURATION to ensure it doesn't exit early on its own
# although we will kill it anyway.
# We also redirect stdin from /dev/null because run_workload.sh uses docker run -it which needs a TTY or will fail without stdin
bash "$SCRIPT_DIR/run_workload.sh" --duration $WORKLOAD_DURATION --network ubuntu_blockchain-net --scenario "exp/uber" --tps 85 --accounts $Nodes --min-balance 10000000000000000000 --rpc http://ubuntu-el-node-0-1:8545 < /dev/null & WORKLOAD_PID=$!
echo "Workload started with PID: $WORKLOAD_PID"

# 5. Wait for the specified duration
echo "Step 5: Running workload for $WORKLOAD_DURATION seconds... + 180 additional seconds"
sleep "$(WORKLOAD_DURATION)"
sleep 180

# 6. Stop run_workload
echo "Step 6: Stopping workload..."
if kill -0 "$WORKLOAD_PID" 2>/dev/null; then
    # Try to kill the process group or the specific docker container if we can find it
    # run_workload.sh starts a docker container. We might want to stop all contender containers.
    # We use a broader filter to ensure we catch it.
    CONTENDER_CONTAINER=$(docker ps --filter "ancestor=z4l3s5i0/contender" --format "{{.ID}}" | head -n 1)
    if [ -n "$CONTENDER_CONTAINER" ]; then
        echo "Stopping contender container $CONTENDER_CONTAINER..."
        docker stop "$CONTENDER_CONTAINER" || true
    fi
    # Use pkill to kill the whole process tree of run_workload.sh if possible
    pkill -P "$WORKLOAD_PID" 2>/dev/null || true
    kill "$WORKLOAD_PID" 2>/dev/null || true
fi
echo "Workload stopped."

# 7. Wait another 600 seconds
echo "Step 7: Waiting 600 seconds for the workload to be processed..."
sleep 600

# 8. Stop manage_logs
echo "Step 8: Stopping log processor..."
bash "$SCRIPT_DIR/manage_logs.sh" stop

# 9. Collect results with sudo
echo "Step 9: Collecting experiment results..."
sudo bash "$SCRIPT_DIR/collect_results.sh"

# 10. Start cleanup
echo "Step 10: Cleaning up..."
bash "$SCRIPT_DIR/cleanup.sh"

echo "=== Experiment Workflow Completed ==="
