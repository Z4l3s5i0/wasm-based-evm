#!/bin/bash
set -e

INSTALL_WASM=false

while [[ "$#" -gt 0 ]]; do
    case $1 in
        --wasm) INSTALL_WASM=true ;;
        *) echo "Unknown parameter passed: $1"; exit 1 ;;
    esac
    shift
done

echo "Updating system..."
sudo apt-get update
sudo dpkg --add-architecture i386
sudo apt-get update
sudo apt-get install -y \
    curl \
    wget \
    git \
    jq \
    build-essential \
    docker.io \
    docker-compose-plugin \
    wine \
    wine32 \
    wine64 \
    libwine \
    libwine:i386 \
    fonts-wine

# 1. Install Wasmer CLI if flag provided
if [ "$INSTALL_WASM" = true ] ; then
    echo "Installing Wasmer CLI..."
    curl https://get.wasmer.io -sSfL | sh
    source ~/.bashrc
fi

echo "Installing Execution client..."
REPO="Z4l3s5i0/wasm-based-evm"
if [ "$INSTALL_WASM" = true ] ;
then

  URL=$(curl -s "https://api.github.com/repos/$REPO/releases" \
    | jq -r '.[0].assets[]
      | select(.name == "wasix-based-evm")
      | .browser_download_url')

  if [ -z "$URL" ]; then
    echo "Native binary not found"
    exit 1
  fi
  echo "Downloading: $URL"
  wget "$URL" -O evm.wasm
  echo "Installed! Run wasmer evm.wasm"

else
  URL=$(curl -s "https://api.github.com/repos/$REPO/releases" \
    | jq -r '.[0].assets[]
          | select(.name | endswith(".wasm") | not)
          | .browser_download_url')

  if [ -z "$URL" ]; then
    echo "No Linux binary found"
    exit 1
  fi

  echo "Downloading: $URL"
  wget "$URL" -O evm
  chmod +x evm

  echo "Installed: Run 'evm'"
fi

# 3. Most recent lighthouse
echo "Installing Lighthouse..."
LH_LATEST_RELEASE=$(curl -s https://api.github.com/repos/sigp/lighthouse/releases/latest \
  | jq -r '.assets[]
    | select(.name | contains("aarch64-unknown-linux-gnu.tar.gz"))
    | .browser_download_url' \
  | head -n 1)
wget "$LH_LATEST_RELEASE" -O lighthouse.tar.gz
tar -xvf lighthouse.tar.gz
sudo mv lighthouse /usr/local/bin/
rm lighthouse.tar.gz

# 4. Most recent release of the ethereum-genesis-generator
echo "Installing Ethereum Genesis Generator..."
git clone https://github.com/ethpandaops/ethereum-genesis-generator.git
cd ethereum-genesis-generator
# It's a python tool or docker based usually. The repo says it can be run via docker.
# But I will just clone it as requested.
cd ..

# 5. Most recent release of the ethstaker-deposit-cli
echo "Installing EthStaker Deposit CLI..."
DEPOSIT_CLI_LATEST=$(curl -s https://api.github.com/repos/ethstaker/ethstaker-deposit-cli/releases/latest \
  | jq -r '.assets[]
    | select(.name | contains("linux-arm64.tar.gz") and (contains(".sha256") | not))
    | .browser_download_url' \
  | head -n 1)
if [ -z "$DEPOSIT_CLI_LATEST" ]; then
    # Fallback to source if no arm64 binary
    git clone https://github.com/ethstaker/ethstaker-deposit-cli.git
    cd ethstaker-deposit-cli
    pip3 install -r requirements.txt
    python3 setup.py install
    cd ..
else
    wget "$DEPOSIT_CLI_LATEST" -O deposit-cli.tar.gz

    tar -xvf deposit-cli.tar.gz

    # find the actual binary (DO NOT assume path)
    BIN=$(find . -type f -name "deposit" | head -n 1)

    if [ -z "$BIN" ]; then
        echo "deposit binary not found after extraction"
        exit 1
    fi

    chmod +x "$BIN"
    sudo mv "$BIN" /usr/local/bin/deposit

    rm deposit-cli.tar.gz
fi

# 6. Install prometheus and node_exporter
echo "Installing Prometheus and Node Exporter..."
# Get latest versions from prometheus.io/download
PROM_VERSION=$(curl -s https://api.github.com/repos/prometheus/prometheus/releases/latest | jq -r .tag_name | sed 's/v//')
NODE_EXP_VERSION=$(curl -s https://api.github.com/repos/prometheus/node_exporter/releases/latest | jq -r .tag_name | sed 's/v//')

wget "https://github.com/prometheus/prometheus/releases/download/v${PROM_VERSION}/prometheus-${PROM_VERSION}.linux-arm64.tar.gz"
tar -xvf "prometheus-${PROM_VERSION}.linux-arm64.tar.gz"
sudo mv "prometheus-${PROM_VERSION}.linux-arm64/prometheus" /usr/local/bin/
sudo mv "prometheus-${PROM_VERSION}.linux-arm64/promtool" /usr/local/bin/
rm -rf "prometheus-${PROM_VERSION}.linux-arm64" "prometheus-${PROM_VERSION}.linux-arm64.tar.gz"

wget "https://github.com/prometheus/node_exporter/releases/download/v${NODE_EXP_VERSION}/node_exporter-${NODE_EXP_VERSION}.linux-arm64.tar.gz"
tar -xvf "node_exporter-${NODE_EXP_VERSION}.linux-arm64.tar.gz"
sudo mv "node_exporter-${NODE_EXP_VERSION}.linux-arm64/node_exporter" /usr/local/bin/
rm -rf "node_exporter-${NODE_EXP_VERSION}.linux-arm64" "node_exporter-${NODE_EXP_VERSION}.linux-arm64.tar.gz"

echo "Installation complete!"
