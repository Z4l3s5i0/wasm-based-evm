#!/bin/bash
set -e

# Multi-Machine Network Orchestrator for v2 Deployment

source "$(dirname "$0")/common.sh"

HOSTS_FILE="hosts.ini"
CHAIN_ID=12345
METRICS_SERVER_URL="http://localhost:9100"
SETUP_REMOTE=false
REGISTRY=""
TAG="latest"

usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  --hosts FILE      Path to hosts configuration (default: hosts.ini)"
    echo "  --chain-id ID     Unique chain ID (default: 12345)"
    echo "  --metrics URL     URL of the metrics server"
    echo "  --setup           Run setup.sh on each host before deployment"
    echo "  --registry URL    Docker registry to pull images from (optional)"
    echo "  --tag TAG         Image tag to use (default: latest)"
    exit 1
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --hosts) HOSTS_FILE="$2"; shift ;;
        --chain-id) CHAIN_ID="$2"; shift ;;
        --metrics) METRICS_SERVER_URL="$2"; shift ;;
        --setup) SETUP_REMOTE=true ;;
        --registry) REGISTRY="$2"; shift ;;
        --tag) TAG="$2"; shift ;;
        *) usage ;;
    esac
    shift
done

if [ ! -f "$HOSTS_FILE" ]; then
    echo "Hosts file $HOSTS_FILE not found."
    exit 1
fi

# 1. Parse hosts and count nodes
nodes=()
while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" =~ ^\[.*\]$ ]] && continue
    [[ -z "$line" ]] && continue
    nodes+=("$line")
done < <(grep -v '^#' "$HOSTS_FILE")

NUM_NODES=${#nodes[@]}
echo "Deploying to $NUM_NODES hosts..."

# 2. Generate Configuration locally
echo "Generating network configuration..."
"$(dirname "$0")/config_gen.sh" --chain-id "$CHAIN_ID" --nodes "$NUM_NODES" --output "startup_v2"

# 3. Deploy to each host
for i in "${!nodes[@]}"; do
    node_info=${nodes[$i]}
    host=$(echo "$node_info" | awk '{print $1}')
    type=$(echo "$node_info" | grep -o 'type=[^ ]*' | cut -d= -f2)
    validator=$(echo "$node_info" | grep -o 'validator=[^ ]*' | cut -d= -f2)
    
    echo "Deploying to node $i: $host (type=$type, validator=$validator)"
    
    # Optional setup
    if [ "$SETUP_REMOTE" = "true" ]; then
        echo "Running setup on $host..."
        scp "$(dirname "$0")/setup.sh" "$host:~/blockchain_setup.sh"
        ssh "$host" "chmod +x ~/blockchain_setup.sh && ~/blockchain_setup.sh && rm ~/blockchain_setup.sh"
    fi

    # Synchronize startup files
    ssh "$host" "mkdir -p blockchain/startup"
    scp startup_v2/genesis.json "$host:blockchain/startup/"
    scp startup_v2/config.yaml "$host:blockchain/startup/"
    scp "startup_v2/jwt_$i.hex" "$host:blockchain/startup/jwt.hex"
    
    # Synchronize validator keys if needed
    if [ "$validator" = "true" ]; then
        ssh "$host" "mkdir -p blockchain/startup/validators"
        scp -r "startup_v2/validators/node_$i" "$host:blockchain/startup/validators/"
    fi
    
    # Start the stack on remote host (assuming Docker is installed)
    IMAGE_NAME="wasm-based-evm-$type"
    COMMAND_ARGS="run --data-dir /app/data --genesis-path /app/startup/genesis.json --peer-name node-$i --auth-rpc-jwt-path /app/startup/jwt.hex --bootstrap-registry $METRICS_SERVER_URL"

    if [ -n "$REGISTRY" ]; then
        IMAGE_NAME="${REGISTRY}/wasix-eth-$type:${TAG}"
        ssh "$host" "docker pull $IMAGE_NAME"
    fi

    ssh "$host" "cd blockchain && \
      docker run -d --name el-node \
        -v \$(pwd)/startup:/app/startup \
        -p 8545:8545 -p 8551:8551 -p 30303:30303 -p 30303:30303/udp \
        $IMAGE_NAME \
        $COMMAND_ARGS && \
      docker run -d --name cl-node \
        -v \$(pwd)/startup:/app/startup \
        -p 5052:5052 -p 9000:9000 \
        sigp/lighthouse lighthouse bn \
        --execution-endpoint http://localhost:8551 \
        --execution-jwt /app/startup/jwt.hex \
        --http --http-address 0.0.0.0 --testnet-dir /app/startup"

    if [ "$validator" = "true" ]; then
        ssh "$host" "cd blockchain && \
          docker run -d --name vc-node \
            -v \$(pwd)/startup:/app/startup \
            --entrypoint sh \
            sigp/lighthouse -c \"lighthouse --testnet-dir /app/startup account validator import --directory /app/startup/validators/node_$i --password-file /app/startup/validators/node_$i/password.txt --datadir /root/.lighthouse --reuse-password && \
            lighthouse vc --beacon-nodes http://localhost:5052 --testnet-dir /app/startup --suggested-fee-recipient 0x0000000000000000000000000000000000000000\""
    fi
    
    # Registration is now handled by the node itself via --bootstrap-registry
done

echo "Deployment complete."
