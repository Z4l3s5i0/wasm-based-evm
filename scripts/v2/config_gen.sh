#!/bin/bash
set -e

# Configuration Generator for v2 Deployment
# Generates genesis, keys, and JWT for a new network experiment

CHAIN_ID=12345
NUM_NODES=4
GENESIS_DELAY=600 # 10 minutes
MNEMONIC="sleep moment list remain like wall lake industry canvas wonder ecology elite duck salad naive syrup frame brass utility club odor country obey pudding"
OUTPUT_DIR="startup_v2"

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --chain-id) CHAIN_ID="$2"; shift ;;
        --nodes) NUM_NODES="$2"; shift ;;
        --output) OUTPUT_DIR="$2"; shift ;;
        *) echo "Unknown parameter: $1"; exit 1 ;;
    esac
    shift
done

mkdir -p "$OUTPUT_DIR"
GENESIS_TIMESTAMP=$(date +%s)
GENESIS_TIMESTAMP=$((GENESIS_TIMESTAMP + GENESIS_DELAY))

echo "Generating config for Chain ID: $CHAIN_ID, Nodes: $NUM_NODES"

# 1. Create values.env for ethereum-genesis-generator
cat <<EOF > "$OUTPUT_DIR/values.env"
PRESET_BASE=mainnet
CHAIN_ID=$CHAIN_ID
EL_AND_CL_MNEMONIC="$MNEMONIC"
SLOT_DURATION_IN_SECONDS=12
NUMBER_OF_VALIDATORS=$NUM_NODES
GENESIS_FORK_VERSION=0x10000000
ALTAIR_FORK_VERSION=0x20000000
BELLATRIX_FORK_VERSION=0x30000000
CAPELLA_FORK_VERSION=0x40000000
ALTAIR_FORK_EPOCH=0
BELLATRIX_FORK_EPOCH=0
TERMINAL_TOTAL_DIFFICULTY=0
CAPELLA_FORK_EPOCH=0
GENESIS_TIMESTAMP=$GENESIS_TIMESTAMP
GENESIS_DELAY=0
GENESIS_GASLIMIT=60000000
SECONDS_PER_ETH1_BLOCK=14
EOF

# 2. Run genesis generator
echo "Running ethereum-genesis-generator..."
docker run --rm -u "$(id -u)" \
  -v "$(pwd)/$OUTPUT_DIR/values.env:/config/values.env" \
  -v "$(pwd)/$OUTPUT_DIR:/data" \
  ethpandaops/ethereum-genesis-generator:master all

# Cleanup and organize files
if [ -d "$OUTPUT_DIR/metadata" ]; then
    mv -f "$OUTPUT_DIR/metadata"/* "$OUTPUT_DIR/"
    rm -rf "$OUTPUT_DIR/metadata"
fi

# 3. Generate JWT secrets for each node
for i in $(seq 0 $((NUM_NODES - 1))); do
    openssl rand -hex 32 > "$OUTPUT_DIR/jwt_$i.hex"
done

# 4. Generate Validator Keys (using lighthouse or deposit-cli)
# For simplicity in this script, we assume the user has lighthouse installed
# or we can use a dockerized version.
echo "Generating validator keys..."
mkdir -p "$OUTPUT_DIR/validators"

# We generate keys for all nodes, even if only 50% use them in a particular experiment.
# This gives us flexibility.
docker run --rm -v "$(pwd)/$OUTPUT_DIR:/data" sigp/lighthouse lighthouse \
  account validator mk-test-net \
  --spec mainnet \
  --validator-count "$NUM_NODES" \
  --testnet-dir /data \
  --node-dir /data/validators

echo "Configuration generated in $OUTPUT_DIR"
