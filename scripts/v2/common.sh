#!/bin/bash

# Shared logic for node deployment

# Assign ports based on node index
# Base ports:
# Eth RPC: 8545
# Auth RPC: 8551
# P2P Discovery: 30303
# P2P Gossip: 30304
# Frontend: 3000
# Metrics: 9050
# Beacon RPC: 5052
# Beacon P2P: 9000

get_eth_rpc_port() { echo $((8545 + $1)); }
get_auth_rpc_port() { echo $((8551 + $1 * 10)); } # Spread them a bit more to avoid conflicts
get_discovery_port() { echo $((30303 + $1 * 2)); }
get_p2p_port() { echo $((30304 + $1 * 2)); }
get_frontend_port() { echo $((3000 + $1)); }
get_metrics_port() { echo $((9050 + $1)); }
get_beacon_rpc_port() { echo $((5052 + $1)); }
get_beacon_p2p_port() { echo $((9000 + $1)); }

# Calculate number of nodes per type based on percentages
calculate_counts() {
    local total=$1
    local p_linux=$2
    local p_windows=$3
    local p_wasix=$4

    local c_linux=$((total * p_linux / 100))
    local c_windows=$((total * p_windows / 100))
    local c_wasix=$((total - c_linux - c_windows)) # Give remainder to wasix

    echo "$c_linux $c_windows $c_wasix"
}

# Determine if a node should be a validator (exactly 50%)
is_validator() {
    local index=$1
    local total=$2
    if [ $((index % 2)) -eq 0 ]; then
        echo "true"
    else
        echo "false"
    fi
}

# Determine the docker-compose command to use
get_compose_cmd() {
    if docker compose version &> /dev/null; then
        echo "docker compose"
    elif docker-compose version &> /dev/null; then
        echo "docker-compose"
    else
        echo "docker compose" # Default to plugin syntax
    fi
}
