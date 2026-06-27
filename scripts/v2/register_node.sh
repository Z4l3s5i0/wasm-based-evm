#!/bin/bash
# Node registration helper

METRICS_SERVER_URL=$1
NODE_ID=$2
NETWORK=$3
CLIENT_TYPE=$4
RPC_URL=$5
METRICS_URL=$6

if [ -z "$METRICS_SERVER_URL" ] || [ -z "$NODE_ID" ]; then
    echo "Usage: $0 <metrics_server_url> <node_id> <network> <client_type> <rpc_url> [metrics_url]"
    exit 1
fi

payload=$(cat <<EOF
{
  "id": "$NODE_ID",
  "network": "$NETWORK",
  "client": "$CLIENT_TYPE",
  "rpc_url": "$RPC_URL",
  "metrics_url": "$METRICS_URL"
}
EOF
)

curl -s -X POST "$METRICS_SERVER_URL/api/nodes/register" \
  -H "Content-Type: application/json" \
  -d "$payload"
