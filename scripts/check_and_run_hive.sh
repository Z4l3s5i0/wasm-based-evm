#!/bin/bash

# Default values
REGISTRY=""
TAG="latest"
INTERVAL=1800 # 30 minutes
ONCE=false

usage() {
    echo "Usage: $0 --registry <registry_url> [--tag <tag>] [--interval <seconds>] [--once]"
    echo "Example: $0 --registry docker.io/myuser"
    exit 1
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --registry) REGISTRY="$2"; shift ;;
        --tag) TAG="$2"; shift ;;
        --interval) INTERVAL="$2"; shift ;;
        --once) ONCE=true ;;
        *) usage ;;
    esac
    shift
done

if [ -z "$REGISTRY" ]; then
    echo "Error: --registry is required."
    usage
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

LINUX_IMAGE="${REGISTRY}/wasix-eth-linux:${TAG}"
WASIX_IMAGE="${REGISTRY}/wasix-eth-wasix:${TAG}"

# Get current image digests
get_digest() {
    local image=$1
    # We use '|| true' to avoid script exit on error if image not found
    docker inspect --format='{{.RepoDigests}}' "$image" 2>/dev/null || true
}

# Pull and check for updates
check_and_pull() {
    local image=$1
    echo "Checking for updates for $image..."
    
    local old_digest=$(get_digest "$image")
    
    # Try to pull the latest image and capture output
    local pull_output
    pull_output=$(docker pull "$image" 2>&1)
    local pull_exit=$?
    
    echo "$pull_output"
    
    if [ $pull_exit -ne 0 ]; then
        echo "Failed to pull $image."
        return 1
    fi
    
    local new_digest=$(get_digest "$image")
    
    # Check if we downloaded something new OR if RepoDigests changed
    if echo "$pull_output" | grep -q "Downloaded newer image" || [ "$old_digest" != "$new_digest" ]; then
        echo "Image $image updated!"
        return 0 # Updated
    fi

    # Extra check for the specific case where the manifest digest changed but docker says "up to date"
    local remote_digest=$(echo "$pull_output" | grep "Digest: " | cut -d' ' -f2 | tr -d '\r')
    if [ -n "$remote_digest" ] && ! echo "$new_digest" | grep -q "$remote_digest"; then
        echo "Image $image updated (Manifest changed)!"
        return 0
    fi
    
    echo "Image $image is up to date."
    return 2 # Not updated
}

run_hive_tests() {
    echo "Starting HIVE tests..."
    
    # 1. Build hive-custom if it doesn't exist
    if [[ "$(docker images -q hive-custom:latest 2> /dev/null)" == "" ]]; then
        echo "Building hive-custom image..."
        docker build -f "$ROOT_DIR/testing/hive.dockerfile" -t hive-custom "$ROOT_DIR"
    fi

    # Ensure workspace directory exists
    mkdir -p "$ROOT_DIR/workspace"

    echo "Running Wasix Wasm-Ethereum Client (wasix-w-eth)..."
    docker run --rm \
      -v /var/run/docker.sock:/var/run/docker.sock \
      -v "$ROOT_DIR/workspace:/hive/workspace" \
      hive-custom \
      --sim ethereum/engine \
      --client wasix-w-eth \
      --sim.parallelism 20

    echo "Running Wasix Rust-Ethereum Client (wasix-r-eth)..."
    docker run --rm \
      -v /var/run/docker.sock:/var/run/docker.sock \
      -v "$ROOT_DIR/workspace:/hive/workspace" \
      hive-custom \
      --sim ethereum/engine \
      --client wasix-r-eth \
      --sim.parallelism 20
}

perform_check() {
    local linux_status
    local wasix_status
    
    check_and_pull "$LINUX_IMAGE"
    linux_status=$?
    
    check_and_pull "$WASIX_IMAGE"
    wasix_status=$?
    
    if [ $linux_status -eq 0 ] || [ $wasix_status -eq 0 ]; then
        echo "One or more images updated. Running HIVE tests."
        run_hive_tests
    else
        echo "No updates found for node images."
    fi
}

if [ "$ONCE" = true ]; then
    perform_check
else
    echo "Starting check loop every $INTERVAL seconds..."
    while true; do
        perform_check
        echo "Waiting for $INTERVAL seconds before next check..."
        sleep "$INTERVAL"
    done
fi
