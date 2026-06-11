#!/bin/bash
set -e

#sudo usermod -aG docker $USER
#newgrp docker

GENESIS_TIMESTAMP=""
NUMBER=0
KEYSTORE_PW="wasmwasmwasm"
MNEMONIC="sleep moment list remain like wall lake industry canvas wonder ecology elite duck salad naive syrup frame brass utility club odor country obey pudding"

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --time) GENESIS_TIMESTAMP="$2"; shift ;;
        --number) NUMBER="$2"; shift ;;
        --keystore-pw) KEYSTORE_PW="$2"; shift ;;
        --mnemonic) MNEMONIC="$2"; shift ;;
        *) echo "Unknown parameter passed: $1"; exit 1 ;;
    esac
    shift
done

if [ -n "$GENESIS_TIMESTAMP" ]; then
    # Convert dd-mm-yyyy HH:MM (CET) → timestamp
    GENESIS_TIMESTAMP=$(TZ="Europe/Zurich" date -d "$GENESIS_TIMESTAMP" +%s 2>/dev/null)

    if [ -z "$GENESIS_TIMESTAMP" ]; then
        echo "Invalid date format. Use: dd-mm-yyyy HH:MM"
        exit 1
    fi
else
    GENESIS_TIMESTAMP=$(date +%s)
fi

echo "Using Genesis Timestamp: $GENESIS_TIMESTAMP"

# 1. Create values.env
cat <<EOF > values.env
PRESET_BASE=mainnet
CHAIN_ID=31133

EL_AND_CL_MNEMONIC="$MNEMONIC"

SLOT_DURATION_IN_SECONDS=12

DEPOSIT_CONTRACT_BLOCK=0x0000000000000000000000000000000000000000000000000000000000000000

# Reduced for devnet
NUMBER_OF_VALIDATORS=2

# -------------------------
# Fork versions
# -------------------------
GENESIS_FORK_VERSION=0x10000000
ALTAIR_FORK_VERSION=0x20000000
BELLATRIX_FORK_VERSION=0x30000000
CAPELLA_FORK_VERSION=0x40000000

# -------------------------
# Capella starts at genesis
# -------------------------
ALTAIR_FORK_EPOCH=0
BELLATRIX_FORK_EPOCH=0
TERMINAL_TOTAL_DIFFICULTY=0
CAPELLA_FORK_EPOCH=0

# -------------------------
# Future forks disabled
# -------------------------
DENEB_FORK_VERSION=0x50000000
DENEB_FORK_EPOCH=18446744073709551615

ELECTRA_FORK_VERSION=0x60000000
ELECTRA_FORK_EPOCH=18446744073709551615

FULU_FORK_VERSION=0x70000000
FULU_FORK_EPOCH=18446744073709551615

GLOAS_FORK_VERSION=0x80000000
GLOAS_FORK_EPOCH=18446744073709551615

HEZE_FORK_VERSION=0x90000000
HEZE_FORK_EPOCH=18446744073709551615

EIP7441_FORK_VERSION=0xa0000000
EIP7441_FORK_EPOCH=18446744073709551615

EIP7928_FORK_VERSION=0xa1000000
EIP7928_FORK_EPOCH=18446744073709551615

# -------------------------
# Genesis settings
# -------------------------
GENESIS_TIMESTAMP=$GENESIS_TIMESTAMP
GENESIS_DELAY=1800 # 30min
GENESIS_GASLIMIT=60000000

SECONDS_PER_ETH1_BLOCK=14
EOF

# 2. Choose ports, lookup IP, provide folders
MY_IP=$(hostname -I | awk '{print $1}')
echo "Detected IP: $MY_IP"

# Define ports
P2P_PORT=9002
DISCOVERY_PORT=9001
ETH_RPC_PORT=8545
AUTH_RPC_PORT=8551
METRICS_PORT=9055
FRONTEND_PORT=3001
BEACON_RPC_PORT=5052
ENR_TCP_PORT=9006
ENR_UDP_PORT=9006

# Create startup directory
mkdir -p startup

# Save config for deployment script
cat <<EOF > deployment_config.env
MY_IP=$MY_IP
P2P_PORT=$P2P_PORT
DISCOVERY_PORT=$DISCOVERY_PORT
ETH_RPC_PORT=$ETH_RPC_PORT
AUTH_RPC_PORT=$AUTH_RPC_PORT
METRICS_PORT=$METRICS_PORT
FRONTEND_PORT=$FRONTEND_PORT
BEACON_RPC_PORT=$BEACON_RPC_PORT
ENR_TCP_PORT=$ENR_TCP_PORT
ENR_UDP_PORT=$ENR_UDP_PORT
EOF

# 3. Generate genesis files
echo "Generating genesis files..."
# Assuming ethereum-genesis-generator is cloned in current dir
REPO_DIR="ethereum-genesis-generator"
CONFIG_DIR="$REPO_DIR/config"

# Ensure config directory exists
if [ ! -d "$CONFIG_DIR" ]; then
    echo "Creating missing config directory..."
    mkdir -p "$CONFIG_DIR"
fi
cp values.env ethereum-genesis-generator/config/values.env
docker run --rm -it -u "$(id -u)" \
  -v $(pwd)/ethereum-genesis-generator/config/values.env:/config/values.env \
  -v $(pwd)/startup:/data \
  ethpandaops/ethereum-genesis-generator:master all

# Ensure files are in startup
mv -f $(pwd)/startup/metadata/* ./startup
rm -rf $(pwd)/startup/metadata
# 4. Generate JWT
openssl rand -hex 32 > startup/jwt_node1.hex
cp startup/jwt_node1.hex startup/jwt_node2.hex

# 5. Run ethstaker deposit command
echo "Running ethstaker deposit command..."
# Ensure validator_keys directory exists
STARTUP_DIR="startup"
VALIDATORS_DIR="$STARTUP_DIR/validator_keys"
if [ ! -d "$VALIDATORS_DIR" ]; then
    echo "Creating missing validator directory..."
    mkdir -p "$VALIDATORS_DIR"
fi
 printf "no\n" | deposit --non_interactive --ignore_connectivity existing-mnemonic --mnemonic "$MNEMONIC" --validator_start_index "$NUMBER" \
--num_validators 1 --chain mainnet --folder startup/validator_keys --amount 1000 --keystore_password "$KEYSTORE_PW" \
--withdrawal_address 0x0000000000000000000000000000000000000001
#--devnet_chain_setting { network_name: , genesis_fork_version: , exit_fork_version: , genesis_validator_root: , multiplier: , min_activation_amount: , min_deposit_amount: }

# 6. Run lighthouse validator to create validator key and deposit file
echo "Creating validator keys with lighthouse..."
lighthouse account validator import --directory startup/validator_keys/ --testnet-dir ./startup --datadir /home/wasm/.lighthouse/node1

echo "Configuration complete!"
