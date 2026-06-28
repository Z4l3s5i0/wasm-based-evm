#!/bin/bash
set -e

# Script to build and push metrics_server to a Docker registry

REGISTRY=""
TAG="latest"

usage() {
    echo "Usage: $0 --registry <registry_url> [--tag <tag>]"
    echo "Example: $0 --registry myuser"
    exit 1
}

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --registry) REGISTRY="$2"; shift ;;
        --tag) TAG="$2"; shift ;;
        *) usage ;;
    esac
    shift
done

if [ -z "$REGISTRY" ]; then
    echo "Error: --registry is required."
    usage
fi

echo "Building and pushing metrics-server..."
METRICS_IMAGE="${REGISTRY}/wasix-eth-metrics:${TAG}"
docker build -t "$METRICS_IMAGE" -f metrics_server/Dockerfile metrics_server
docker push "$METRICS_IMAGE"

echo "Process completed for metrics-server at ${REGISTRY} with tag ${TAG}"
