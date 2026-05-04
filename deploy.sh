#!/bin/bash
set -e

USE_WASM=false

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --wasm) USE_WASM=true ;;
        *) echo "Unknown parameter passed: $1"; exit 1 ;;
    esac
    shift
done

if [ ! -f deployment_config.env ]; then
    echo "deployment_config.env not found. Please run configure.sh first."
    exit 1
fi

source deployment_config.env

# 1. Run Node Exporter
echo "Starting Node Exporter..."
nohup node_exporter > node_exporter.log 2>&1 &

# 2. Run Prometheus
echo "Starting Prometheus..."
cat <<EOF > prometheus.yml
global:
  scrape_interval: 15s
scrape_configs:
  - job_name: 'node_exporter'
    static_configs:
      - targets: ['localhost:9100']
  - job_name: 'wasm-evm'
    static_configs:
      - targets: ['localhost:$METRICS_PORT']
EOF
nohup prometheus --config.file=prometheus.yml > prometheus.log 2>&1 &

# 3. Run Execution Client
echo "Starting Execution Client..."
if [ "$USE_WASM" = true ]; then
    echo "Running via Wasmer..."
    # Using the command from the issue description, adapted with variables
    nohup wasmer run wasix-based-evm.wasi.wasm --enable-threads --net --volume ./startup:./startup -- \
        --data-dir ./startup --ext-ip $MY_IP --verbose 1 --genesis-path ./startup/genesis.json \
        --peer-name node1 --metrics-port $METRICS_PORT --bootnodes $MY_IP:$DISCOVERY_PORT \
        --auth-rpc-port $AUTH_RPC_PORT --eth-rpc-port $ETH_RPC_PORT --p2p-port $P2P_PORT \
        --discovery-port $DISCOVERY_PORT --frontend-port $FRONTEND_PORT > execution_client.log 2>&1 &
else
    echo "Running via executable..."
    nohup ./wasix-based-evm --data-dir ./startup --ext-ip $MY_IP --verbose 1 \
        --genesis-path ./startup/genesis.json --peer-name node1 --metrics-port $METRICS_PORT \
        --bootnodes $MY_IP:$DISCOVERY_PORT --auth-rpc-port $AUTH_RPC_PORT --eth-rpc-port $ETH_RPC_PORT \
        --p2p-port $P2P_PORT --discovery-port $DISCOVERY_PORT --frontend-port $FRONTEND_PORT > execution_client.log 2>&1 &
fi

# 4. Run Lighthouse Beacon Node
echo "Starting Lighthouse Beacon Node..."
# Using the command from the issue description, adapted with variables
# Note: The issue description had a hardcoded boot-node ENR and 192.168.1.156, I'll use the variables where appropriate
# but I'll keep the ENR from the example as it might be a specific testnet bootnode.
BOOT_NODES="enr:-N24QJD-RgL2EcbbXi4mkV_JSDzBjDJlT5VJeJt-vTJLaVl-ZERDq80c8E9ZhyS0BugwGmGi5D_gjwTpLQf40Fbt2WkBh2F0dG5ldHOIAAAAAAAAAACGY2xpZW500YpMaWdodGhvdXNlhTguMS4zhGV0aDKQFBGi7kAAAAD__________4JpZIJ2NIJpcITAqAGYhHF1aWOCIymJc2VjcDI1NmsxoQMqXe3FA3PUHS8XG3xqYSGM-HJURwBOQ2P2pj5XOuQDLohzeW5jbmV0cwCDdGNwgiMtg3VkcIIjLQ"

nohup lighthouse bn \
    --execution-endpoint http://localhost:$AUTH_RPC_PORT \
    --execution-jwt ./startup/jwt_node1.hex \
    --http \
    --testnet-dir ./startup \
    --boot-nodes "$BOOT_NODES" \
    --debug-level debug \
    --enr-address $MY_IP \
    --enr-tcp-port $ENR_TCP_PORT \
    --enr-udp-port $ENR_UDP_PORT \
    --datadir /home/wasm/.lighthouse/node2 > lighthouse_bn.log 2>&1 &

# 5. Run Lighthouse Validator Client
echo "Starting Lighthouse Validator Client..."
nohup lighthouse vc \
    --beacon-nodes http://localhost:$BEACON_RPC_PORT \
    --testnet-dir ./startup \
    --datadir /home/wasm/.lighthouse/node1 \
    --debug-level debug \
    --suggested-fee-recipient 0x0000000000000000000000000000000000000001 > lighthouse_vc.log 2>&1 &

echo "Deployment started! Check logs for details."
