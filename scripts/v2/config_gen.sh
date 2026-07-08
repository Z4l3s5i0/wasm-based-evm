#!/bin/bash
set -e

# Configuration Generator for v2 Deployment
# Generates genesis, keys, and JWT for a new network experiment

CHAIN_ID=12345
NUM_NODES=4
GENESIS_DELAY=180 # 3 minutes
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
CAPELLA_FORK_EPOCH=99999999
DENEB_FORK_EPOCH=99999999
ELECTRA_FORK_EPOCH=99999999
FULU_FORK_EPOCH=99999999
GENESIS_TIMESTAMP=$GENESIS_TIMESTAMP
GENESIS_DELAY=$GENESIS_DELAY
GENESIS_GASLIMIT=6000000000
# Pre-fund the default contender account (first Anvil account)
# and a few accounts from the custom mnemonic if needed.
# 100000 ETH = 100000000000000000000000 Wei
EL_PREMINE_ADDRS='{"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266": "100000000000000000000000"}'
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

# 4. Generate Validator Keys (using ethstaker-deposit-cli)
echo "Generating validator keys using ethstaker-deposit-cli..."
mkdir -p "$OUTPUT_DIR/validator_keys"
echo "password12345" > "$OUTPUT_DIR/password.txt"

# Run deposit-cli
# We use a dockerized version for consistency. 
# Note: existing-mnemonic is used to derive keys from the shared MNEMONIC.
# We pipe the password and index twice to handle confirmation prompts.
# We run as the current user to avoid permission issues with generated files.
printf "%s\n%s\n\n" "0" "password12345" | \
docker run --rm -i -u "$(id -u):$(id -g)" -v "$(pwd)/$OUTPUT_DIR:/data" ghcr.io/ethstaker/ethstaker-deposit-cli:latest --language English existing-mnemonic \
  --mnemonic="$MNEMONIC" \
  --num_validators="$NUM_NODES" \
  --validator_start_index=0 \
  --mnemonic_language="english" \
  --chain=mainnet \
  --keystore_password="password12345" \
  --folder=/data

# Organize keys into node-specific folders for compatibility with start_remote.sh
mkdir -p "$OUTPUT_DIR/validators"
# Fix permissions for the generated keys
chmod -R 755 "$OUTPUT_DIR/validator_keys"
for i in $(seq 0 $((NUM_NODES - 1))); do
    NODE_DIR="$OUTPUT_DIR/validators/node_$i"
    mkdir -p "$NODE_DIR"
    # In a real scenario, we'd distribute keys properly. 
    # Here we just put one key per node for simplicity, or all if preferred.
    # To match lighthouse generate behavior, we'll put all keys in each node or 
    # distribute them. The lighthouse command used --count $NUM_NODES for each? 
    # No, it generated $NUM_NODES total.
    
    # Let's copy the entire validator_keys to each node for now, 
    # or just keep them in validator_keys and let the orchestrator handle it.
    # The start_remote.sh expects node_$i to have the keys.
done

# Actually, let's just move the generated keys to a common place and 
# let the user/orchestrator decide. 
# But to keep start_remote.sh working:
if [ -d "$OUTPUT_DIR/validator_keys" ]; then
    # Distribute keys: node_0 gets first key, node_1 gets second, etc.
    KEYS=($(ls "$OUTPUT_DIR/validator_keys"/keystore-m_*.json))
    for i in $(seq 0 $((NUM_NODES - 1))); do
        if [ $i -lt ${#KEYS[@]} ]; then
            mkdir -p "$OUTPUT_DIR/validators/node_$i"
            cp "${KEYS[$i]}" "$OUTPUT_DIR/validators/node_$i/"
            cp "$OUTPUT_DIR/password.txt" "$OUTPUT_DIR/validators/node_$i/"
        fi
    done
fi

echo "Configuration generated in $OUTPUT_DIR"
