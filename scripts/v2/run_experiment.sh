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
RATIO2=$((100 - RATIO))

echo "=== Starting Experiment Workflow ==="
echo "Chain ID: $CHAIN_ID"
echo "Workload Duration: $WORKLOAD_DURATION seconds"
echo "Nodes: $NODES"
echo "RATIO: $RATIO Rust-compiled, $RATIO2 WASM-compiled"

# 1. Start the local network
echo "Step 1: Starting local network..."
bash "$SCRIPT_DIR/start_local.sh" --nodes "$NODES" --chain-id "$CHAIN_ID" --linux "$RATIO" --wasix "$RATIO2" --setup --cleanup --registry docker.io/z4l3s5i0

# 2. Start manage_logs
echo "Step 2: Starting background log processor..."
bash "$SCRIPT_DIR/manage_logs.sh" start --chain-id "$CHAIN_ID"

# 3. Wait for 180 seconds
echo "Step 3: Waiting 180 seconds for network stabilization..."
sleep 180

# 4. Start run_workload for each node in background
echo "Step 4: Starting workload for each node in background..."
MNEMONIC="sleep moment list remain like wall lake industry canvas wonder ecology elite duck salad naive syrup frame brass utility club odor country obey pudding"
WORKLOAD_PIDS=()
BASE_PORT=8545
for i in $(seq 0 $((NODES - 1))); do
    RPC_PORT=$((BASE_PORT + i))
    RPC_URL="http://experiments-el-node-$i-1:$RPC_PORT"
    
    # Deriving private key for the account index i
    echo "Deriving private key for node $i..."
    PRIV_KEY=$(docker run --rm ghcr.io/foundry-rs/foundry:latest 'cast wallet private-key "$MNEMONIC" $i' | grep -i "Private Key:" | sed 's/.*\(0x[0-9a-fA-F]*\).*/\1/')

    # Validate PRIV_KEY length to avoid HexError(InvalidStringLength)
    if [ ${#PRIV_KEY} -ne 66 ]; then
        echo "Error: Invalid private key derived for node $i: '$PRIV_KEY'"
        exit 1
    fi

    echo "Launching workload for node $i at $RPC_URL"
    # We pass --duration slightly longer than WORKLOAD_DURATION to ensure it doesn't exit early on its own
    # although we will kill it anyway.
    # We also redirect stdin from /dev/null because run_workload.sh uses docker run -it which needs a TTY or will fail without stdin
    bash "$SCRIPT_DIR/run_workload.sh" \
        --pk "$PRIV_KEY" \
        --duration $((WORKLOAD_DURATION)) \
        --network experiments_blockchain-net \
        --scenario "exp/uber" \
        --tps 80 \
        --accounts 10 \
        --min-balance 10000000000000000000 \
        --rpc "$RPC_URL" < /dev/null &
    WORKLOAD_PIDS+=($!)
done
echo "Workloads started with PIDs: ${WORKLOAD_PIDS[*]}"

# 5. Wait for the specified duration
echo "Step 5: Running workload for $WORKLOAD_DURATION seconds... + 900 additional seconds for workload to be processed"
sleep "$WORKLOAD_DURATION"
sleep 900

# 6. Stop run_workload
echo "Step 6: Stopping workloads..."
for PID in "${WORKLOAD_PIDS[@]}"; do
    if kill -0 "$PID" 2>/dev/null; then
        # Use pkill to kill the whole process tree of run_workload.sh if possible
        pkill -P "$PID" 2>/dev/null || true
        kill "$PID" 2>/dev/null || true
    fi
done

# Also stop all contender containers
CONTENDER_CONTAINERS=$(docker ps --filter "ancestor=z4l3s5i0/contender" --format "{{.ID}}")
if [ -n "$CONTENDER_CONTAINERS" ]; then
    echo "Stopping contender containers..."
    docker stop $CONTENDER_CONTAINERS || true
fi
echo "Workloads stopped."

# 7. Wait another 300 seconds
echo "Step 7: Waiting additional 300 seconds for the workload to be processed..."
sleep 300

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
