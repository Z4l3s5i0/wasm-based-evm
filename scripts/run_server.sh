#!/bin/bash
set -e

# Script to run Contender Server using Docker

# Default values
RPC_HOST="0.0.0.0:3700"
SSE_HOST="0.0.0.0:3701"
CONTENDER_IMAGE="docker.io/z4l3s5i0/contender:latest"
STATE_DIR="$(pwd)/.contender_state"
NETWORK=""

usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  --rpc-host HOST  Address and port for JSON-RPC server (default: $RPC_HOST)"
    echo "  --sse-host HOST  Address and port for Web UI / SSE server (default: $SSE_HOST)"
    echo "  --image NAME     Contender Docker image name (default: $CONTENDER_IMAGE)"
    echo "  --state-dir DIR  Directory to store contender state (default: $STATE_DIR)"
    echo "  --network NAME   Docker network to join"
    exit 1
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --rpc-host) RPC_HOST="$2"; shift ;;
        --sse-host) SSE_HOST="$2"; shift ;;
        --image) CONTENDER_IMAGE="$2"; shift ;;
        --state-dir) STATE_DIR="$2"; shift ;;
        --network) NETWORK="$2"; shift ;;
        *) usage ;;
    esac
    shift
done

# Ensure state directory exists
mkdir -p "$STATE_DIR"
STATE_DIR=$(realpath "$STATE_DIR")

echo "--- Contender Server Configuration ---"
echo "RPC Host: $RPC_HOST"
echo "SSE Host: $SSE_HOST"
echo "Image: $CONTENDER_IMAGE"
echo "State Dir: $STATE_DIR"
if [ -n "$NETWORK" ]; then
    echo "Network: $NETWORK"
fi
echo "--------------------------------------"

# Extract ports for mapping
RPC_PORT=$(echo "$RPC_HOST" | cut -d':' -f2)
SSE_PORT=$(echo "$SSE_HOST" | cut -d':' -f2)

DOCKER_OPTS="-it --rm"
if [ -n "$NETWORK" ]; then
    DOCKER_OPTS="$DOCKER_OPTS --network $NETWORK"
fi

# Run contender server
docker run $DOCKER_OPTS \
    -p "$RPC_PORT:$RPC_PORT" \
    -p "$SSE_PORT:$SSE_PORT" \
    -v "$STATE_DIR":/root/.local/state/contender \
    -e RPC_HOST="$RPC_HOST" \
    -e SSE_HOST="$SSE_HOST" \
    "$CONTENDER_IMAGE" server

