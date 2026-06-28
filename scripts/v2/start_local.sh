#!/bin/bash
set -e

# Local Docker Network Orchestrator for v2 Deployment

source "$(dirname "$0")/common.sh"

NODES=4
LINUX_RATIO=50
WINDOWS_RATIO=25
WASIX_RATIO=25
CHAIN_ID=12345
CLEANUP=false
SETUP=false
REGISTRY=""
TAG="latest"

usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  --nodes N         Total number of nodes (default: 4)"
    echo "  --linux P         Percentage of Linux nodes (default: 50)"
    echo "  --windows P       Percentage of Windows nodes (default: 25)"
    echo "  --wasix P         Percentage of Wasix nodes (default: 25)"
    echo "  --chain-id ID     Unique chain ID (default: 12345)"
    echo "  --cleanup         Remove existing data before starting"
    echo "  --setup           Run setup.sh locally before starting"
    echo "  --registry URL    Docker registry to pull images from (optional)"
    echo "  --tag TAG         Image tag to use (default: latest)"
    exit 1
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --nodes) NODES="$2"; shift ;;
        --linux) LINUX_RATIO="$2"; shift ;;
        --windows) WINDOWS_RATIO="$2"; shift ;;
        --wasix) WASIX_RATIO="$2"; shift ;;
        --chain-id) CHAIN_ID="$2"; shift ;;
        --cleanup) CLEANUP=true ;;
        --setup) SETUP=true ;;
        --registry) REGISTRY="$2"; shift ;;
        --tag) TAG="$2"; shift ;;
        *) usage ;;
    esac
    shift
done

if [ "$SETUP" = true ]; then
    echo "Running local setup..."
    "$(dirname "$0")/setup.sh"
fi

if [ "$CLEANUP" = true ]; then
    echo "Cleaning up old data..."
  # Stop running containers first to release file locks
    COMPOSE_CMD=$(get_compose_cmd)
    if [ -f "$COMPOSE_FILE" ]; then
        $COMPOSE_CMD -f "$COMPOSE_FILE" down --volumes --remove-orphans || true
    fi

    # Use sudo to force-remove files owned by root
    sudo rm -rf startup_v2 data_v2fi
fi

# 1. Generate Configuration
echo "Generating network configuration..."
"$(dirname "$0")/config_gen.sh" --chain-id "$CHAIN_ID" --nodes "$NODES" --output "startup_v2"

# 2. Calculate node counts
read c_linux c_windows c_wasix <<< $(calculate_counts "$NODES" "$LINUX_RATIO" "$WINDOWS_RATIO" "$WASIX_RATIO")
echo "Node counts: Linux=$c_linux, Windows=$c_windows, Wasix=$c_wasix"

# 3. Generate docker-compose.yml
COMPOSE_FILE="docker-compose.v2.yml"

# Prepare variables for template replacement
if [ -n "$REGISTRY" ]; then
    export METRICS_IMAGE="${REGISTRY}/wasix-eth-metrics:${TAG}"
    export METRICS_BUILD_SECTION="# Registry image used"
else
    export METRICS_IMAGE="wasix-eth-metrics:latest"
    export METRICS_BUILD_SECTION="build:
      context: ./metrics_server
      dockerfile: Dockerfile"
fi

# Use envsubst if available, otherwise just copy and manual replace (simplified for script)
# Using sed for compatibility
sed -e "s|\${METRICS_IMAGE:-wasix-eth-metrics:latest}|$METRICS_IMAGE|g" \
    -e "s|\${METRICS_BUILD_SECTION:-# No build section}|$METRICS_BUILD_SECTION|g" \
    "$(dirname "$0")/templates/docker-compose.yml.template" > "$COMPOSE_FILE"

# Track assigned types
node_types=()
for i in $(seq 1 $c_linux); do node_types+=("linux"); done
for i in $(seq 1 $c_windows); do node_types+=("windows"); done
for i in $(seq 1 $c_wasix); do node_types+=("wasix"); done

for i in $(seq 0 $((NODES - 1))); do
    TYPE=${node_types[$i]}
    IS_VAL=$(is_validator "$i" "$NODES")
    
    ETH_PORT=$(get_eth_rpc_port "$i")
    AUTH_PORT=$(get_auth_rpc_port "$i")
    DISC_PORT=$(get_discovery_port "$i")
    P2P_PORT=$(get_p2p_port "$i")
    FE_PORT=$(get_frontend_port "$i")
    METRICS_PORT=$(get_metrics_port "$i")
    BN_RPC_PORT=$(get_beacon_rpc_port "$i")
    BN_P2P_PORT=$(get_beacon_p2p_port "$i")
    
    PEER_NAME="${TYPE}-node-${i}"
    DATA_DIR="./data_v2/node-${i}"
    mkdir -p "$DATA_DIR/el" "$DATA_DIR/cl"

    # Append Execution Client
    COMMAND_STR="command: [\"run\", \"--data-dir\", \"/app/data\", \"--genesis-path\", \"/app/startup/genesis.json\", \"--peer-name\", \"$PEER_NAME\", \"--eth-rpc-port\", \"$ETH_PORT\", \"--auth-rpc-port\", \"$AUTH_PORT\", \"--p2p-port\", \"$P2P_PORT\", \"--discovery-port\", \"$DISC_PORT\", \"--frontend-port\", \"$FE_PORT\", \"--metrics-port\", \"$METRICS_PORT\", \"--auth-rpc-jwt-path\", \"/app/startup/jwt_$i.hex\", \"--bootstrap-registry\", \"http://metrics-server:9100\"]"

    if [ "$TYPE" = "windows" ]; then
        # Ensure we always use the full path to wine in the command if it's being overridden or used as argument
        # Actually, since it's an argument to the ENTRYPOINT, it should NOT include wine again.
        # But if for some reason ENTRYPOINT is bypassed, we might have issues.
        # We'll stick to the args and ensure the ENTRYPOINT is correct in the Dockerfile.
        :
    fi

    if [ -n "$REGISTRY" ]; then
        IMAGE_STR="image: ${REGISTRY}/wasix-eth-${TYPE}:${TAG}"
        BUILD_STR="# Using registry image"
    else
        IMAGE_STR="image: wasix-eth-${TYPE}:latest"
        BUILD_STR="build:
      context: ./wasix_eth
      dockerfile: Dockerfile.$TYPE"
    fi

    cat <<EOF >> "$COMPOSE_FILE"
  el-node-$i:
    $IMAGE_STR
    $BUILD_STR
    volumes:
      - $DATA_DIR/el:/app/data
      - ./startup_v2:/app/startup
    ports:
      - "$ETH_PORT:$ETH_PORT"
      - "$AUTH_PORT:$AUTH_PORT"
      - "$P2P_PORT:$P2P_PORT"
      - "$DISC_PORT:$DISC_PORT/udp"
      - "$FE_PORT:$FE_PORT"
      - "$METRICS_PORT:$METRICS_PORT"
    networks:
      - blockchain-net
    $COMMAND_STR

EOF

    # Append Consensus Client (Lighthouse Beacon Node)
    cat <<EOF >> "$COMPOSE_FILE"
  cl-node-$i:
    image: sigp/lighthouse
    volumes:
      - $DATA_DIR/cl:/root/.lighthouse
      - ./startup_v2:/app/startup
    ports:
      - "$BN_RPC_PORT:5052"
      - "$BN_P2P_PORT:9000"
    networks:
      - blockchain-net
    depends_on:
      - el-node-$i
    command: >
      lighthouse bn
      --execution-endpoint http://el-node-$i:$AUTH_PORT
      --execution-jwt /app/startup/jwt_$i.hex
      --http
      --http-address 0.0.0.0
      --testnet-dir /app/startup
      --debug-level info
      --datadir /root/.lighthouse
      --enr-udp-port 9000
      --enr-tcp-port 9000
      --discovery-port 9000
      --port 9000

EOF

    # Append Validator Client if applicable
    if [ "$IS_VAL" = "true" ]; then
        cat <<EOF >> "$COMPOSE_FILE"
  vc-node-$i:
    image: sigp/lighthouse
    volumes:
      - $DATA_DIR/cl:/root/.lighthouse
      - ./startup_v2:/app/startup
    networks:
      - blockchain-net
    depends_on:
      - cl-node-$i
    entrypoint:
      - sh
      - -c
      - |
        lighthouse --testnet-dir /app/startup account validator import --directory /app/startup/validators/node_$i --password-file /app/startup/validators/node_$i/password.txt --datadir /root/.lighthouse --reuse-password &&
        lighthouse vc --beacon-nodes http://cl-node-$i:5052 --testnet-dir /app/startup --datadir /root/.lighthouse --debug-level info --suggested-fee-recipient 0x0000000000000000000000000000000000000000 --init-slashing-protection
EOF
    fi
done

echo "Generated $COMPOSE_FILE"
echo "Starting network (forcing rebuild)..."
COMPOSE_CMD=$(get_compose_cmd)
$COMPOSE_CMD -f "$COMPOSE_FILE" up -d --build

echo "Network started. Use '$COMPOSE_CMD -f $COMPOSE_FILE logs -f' to see logs."
