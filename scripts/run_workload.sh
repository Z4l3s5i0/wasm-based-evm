#!/bin/bash
set -e

# Workload Script for Testing Network using Contender

# Default values
RPC_URL="http://host.docker.internal:8545"
PRIVATE_KEY="ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
SCENARIO="stress"
IS_CAMPAIGN=false
TPS=10
DURATION=60
ACCOUNTS=10
MIN_BALANCE="1 ether"
GAS_LIMIT=""
NETWORK=""
CONTENDER_IMAGE="docker.io/z4l3s5i0/contender:latest"
STATE_DIR="$(pwd)/.contender_state"
SKIP_FUNDING=false

usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  --rpc URL        RPC URL of the target node (default: $RPC_URL)"
    echo "  --pk KEY         Private key for transaction signing (required for setup)"
    echo "  --scenario NAME  Scenario name: stress, simple, uniV2, etc. (default: $SCENARIO)"
    echo "  --campaign NAME  Campaign name (if set, --scenario is ignored)"
    echo "  --tps N          Transactions per second (for single scenario) (default: $TPS)"
    echo "  --duration N     Duration in seconds (for single scenario) (default: $DURATION)"
    echo "  --accounts N     Number of accounts per agent (default: $ACCOUNTS)"
    echo "  --min-balance B  Minimum balance for accounts (default: $MIN_BALANCE)"
    echo "  --gas-limit N    Gas limit override for transactions"
    echo "  --network NAME   Docker network to join"
    echo "  --image NAME     Contender Docker image name (default: $CONTENDER_IMAGE)"
    echo "  --state-dir DIR  Directory to store contender state (default: $STATE_DIR)"
    echo "  --skip-funding   Skip funding accounts (passed to contender)"
    exit 1
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --rpc) RPC_URL="$2"; shift ;;
        --pk) PRIVATE_KEY="$2"; shift ;;
        --scenario) SCENARIO="$2"; shift ;;
        --campaign) SCENARIO="$2"; IS_CAMPAIGN=true; shift ;;
        --tps) TPS="$2"; shift ;;
        --duration) DURATION="$2"; shift ;;
        --accounts) ACCOUNTS="$2"; shift ;;
        --min-balance) MIN_BALANCE="$2"; shift ;;
        --gas-limit) GAS_LIMIT="$2"; shift ;;
        --network) NETWORK="$2"; shift ;;
        --image) CONTENDER_IMAGE="$2"; shift ;;
        --state-dir) STATE_DIR="$2"; shift ;;
        --skip-funding) SKIP_FUNDING=true ;;
        *) usage ;;
    esac
    shift
done

# Ensure state directory exists
mkdir -p "$STATE_DIR"

# Convert relative state dir to absolute
STATE_DIR=$(realpath "$STATE_DIR")

echo "--- Contender Workload Configuration ---"
echo "RPC URL: $RPC_URL"
if [ "$IS_CAMPAIGN" = true ]; then
    echo "Campaign: $SCENARIO"
else
    echo "Scenario: $SCENARIO"
    echo "TPS: $TPS"
    echo "Duration: $DURATION seconds"
fi
echo "Accounts per agent: $ACCOUNTS"
echo "Min Balance: $MIN_BALANCE"
if [ -n "$GAS_LIMIT" ]; then
    echo "Gas Limit: $GAS_LIMIT"
fi
if [ -n "$NETWORK" ]; then
    echo "Network: $NETWORK"
fi
echo "State Dir: $STATE_DIR"
echo "---------------------------------------"

# Reference string
if [ "$IS_CAMPAIGN" = true ]; then
    SCENARIO_REF="/campaigns/$SCENARIO.toml"
else
    SCENARIO_REF="/scenarios/$SCENARIO.toml"
fi

EXTRA_ARGS=""
DOCKER_OPTS="-it"
# Check for network
if [ -n "$NETWORK" ]; then
    DOCKER_OPTS="$DOCKER_OPTS --network $NETWORK"

    # If using custom network and default RPC URL, host.docker.internal might not work
    # We should suggest using a container name or try to resolve it.
    if [[ "$RPC_URL" == *"host.docker.internal"* ]]; then
        echo "Warning: Using 'host.docker.internal' with a custom Docker network might fail on Linux."
        # Try to find a node container in this network
        # The containers in start_local.sh are named el-node-0, el-node-1, etc.
        # But they might have a prefix if docker-compose is used (e.g. wasm_el-node-0)
        # We search for any container that contains 'el-node' and is in the specified network
        NODE_CONTAINER=$(docker ps --filter "network=$NETWORK" --format "{{.Names}}" | grep "el-node" | head -n 1)
        if [ -n "$NODE_CONTAINER" ]; then
            # We need to know the port.
            # In v2, el-node-0 is 8545, el-node-1 is 8546, etc.
            # However, INSIDE the docker network, the service name is el-node-0, el-node-1, etc.
            # and it listens on the port specified by --eth-rpc-port in FLAGS.
            
            # If the container name is wasm-el-node-1-1, it probably corresponds to el-node-1.
            # Let's try to extract the node index from the container name.
            # Naming pattern: [prefix]el-node-[index](-[instance])?
            NODE_INDEX=$(echo "$NODE_CONTAINER" | grep -oE "el-node-[0-9]+" | cut -d'-' -f3)
            
            if [ -n "$NODE_INDEX" ]; then
                # Port is 8545 + node_index
                PORT=$((8545 + NODE_INDEX))
                NEW_RPC_URL="http://${NODE_CONTAINER}:${PORT}"
            else
                # Fallback to extracting port from original RPC_URL or use 8545
                PORT=$(echo "$RPC_URL" | sed -e 's/.*:\([0-9]\+\).*/\1/' | grep -E '^[0-9]+$' || echo "8545")
                NEW_RPC_URL="http://${NODE_CONTAINER}:${PORT}"
            fi
            
            echo "Suggested RPC URL for this network: $NEW_RPC_URL"
            echo "Switching to $NEW_RPC_URL..."
            RPC_URL="$NEW_RPC_URL"
        fi
    fi
fi

if [ -n "$GAS_LIMIT" ]; then
    # We use --env to override gas limit if it's supported by scenario,
    # but contender doesn't have a direct --gas-limit CLI flag for all txs.
    # However, for Uber scenario we might need it.
    # If we can't do it via CLI, we might need to patch the TOML.
    # Let's try --env GAS_LIMIT=$GAS_LIMIT
    EXTRA_ARGS="$EXTRA_ARGS --env GAS_LIMIT=$GAS_LIMIT"
fi

if [ "$SKIP_FUNDING" = true ]; then
    EXTRA_ARGS="$EXTRA_ARGS --skip-funding"
fi

if [ -n "$MIN_BALANCE" ]; then
    EXTRA_ARGS="$EXTRA_ARGS --min-balance $MIN_BALANCE"
fi

# Run setup if private key is provided
#if [ -n "$PRIVATE_KEY" ]; then
#    echo "Running contender setup..."
#    docker run $DOCKER_OPTS \
#        -v "$STATE_DIR":/root/.local/state/contender \
#        -w / \
#        "$CONTENDER_IMAGE" setup "$SCENARIO_REF" -r "$RPC_URL" -p "$PRIVATE_KEY" -a "$ACCOUNTS" $EXTRA_ARGS
#fi

# Run workload
if [ "$IS_CAMPAIGN" = true ]; then
    echo "Starting workload (campaign)..."
    docker run $DOCKER_OPTS \
        -v "$STATE_DIR":/root/.local/state/contender \
        -w / \
        "$CONTENDER_IMAGE" campaign "$SCENARIO_REF" -r "$RPC_URL" -p "$PRIVATE_KEY" -a "$ACCOUNTS" $EXTRA_ARGS --pending-timeout 36
else
    echo "Starting workload (spam)..."
    docker run $DOCKER_OPTS \
        -v "$STATE_DIR":/root/.local/state/contender \
        -w / \
        "$CONTENDER_IMAGE" spam "$SCENARIO_REF" -r "$RPC_URL" --tps "$TPS" -d "$DURATION" -p "$PRIVATE_KEY" -a "$ACCOUNTS" $EXTRA_ARGS --pending-timeout 36
fi

echo "Workload finished."

#
#wasm@test1:~$ ./eth-dev/run_workload.sh --network wasm_blockchain-net --campaign uber_workload --accounts 6 --min-balance 10000000000000000000 --rpc http://wasm-el-node-0-1:8545
#--- Contender Workload Configuration ---
#RPC URL: http://wasm-el-node-0-1:8545
#Campaign: uber_workload
#Accounts per agent: 6
#Min Balance: 10000000000000000000
#Network: wasm_blockchain-net
#State Dir: /home/wasm/.contender_state
#---------------------------------------
#Starting workload (campaign)...
#2026-07-08T20:26:50.498996Z  INFO data directory: /root/.local/state/contender
#2026-07-08T20:26:50.502826Z  INFO connecting to http://wasm-el-node-0-1:8545/
#2026-07-08T20:26:50.520795Z  INFO Funding agent accounts from 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
#2026-07-08T20:26:51.569164Z  INFO simulate_setup_cost: beginning setup simulation
#2026-07-08T20:26:51.662777Z  INFO simulate_setup_cost: anvil ready at http://localhost:35135/
#2026-07-08T20:26:51.702622Z  INFO simulate_setup_cost: deploying contract: "ContractUber"
#deploying contract with wallet address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
#2026-07-08T20:26:58.742898Z  INFO simulate_setup_cost: syncing nonces from RPC...
#2026-07-08T20:26:58.744757Z  INFO simulate_setup_cost: setup simulation complete. estimated setup cost: 0.000000000000000000 ether
#2026-07-08T20:26:58.853546Z  INFO Deploying contracts...
#2026-07-08T20:26:58.854706Z  INFO deploying contract: "ContractUber"
#deploying contract with wallet address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
#Error:   x core error
#  |-> failed to find pending tx
#  `-> transaction was not confirmed within the timeout

