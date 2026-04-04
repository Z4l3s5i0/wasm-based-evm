#!/bin/bash

# Usage:
# ./prepare_deploy.sh artifacts/contracts/HelloValue.sol/HelloValue.json "Hello Blockchain"

JSON_FILE="$1"
CONSTRUCTOR_ARG="$2"

if [ -z "$JSON_FILE" ] || [ -z "$CONSTRUCTOR_ARG" ]; then
  echo "Usage: $0 <path_to_HelloValue.json> <constructor_string>"
  exit 1
fi


# Step 1: Extract bytecode from JSON
BYTECODE=$(jq -r '.bytecode' "$JSON_FILE")

if [ -z "$BYTECODE" ] || [ "$BYTECODE" = "0x" ]; then
  echo "Error: bytecode not found in JSON file."
  exit 1
fi

# Step 2: ABI encode constructor using Node.js + ethers
CONSTRUCTOR_HEX=$(node -e "const ethers = require('ethers'); console.log(ethers.AbiCoder.defaultAbiCoder().encode(['string'], ['$CONSTRUCTOR_ARG']).slice(2));")

# Step 3: Concatenate bytecode + constructor
DEPLOY_DATA="0x$BYTECODE$CONSTRUCTOR_HEX"

# Step 4: Output result
echo "=== DEPLOY DATA ==="
echo "$DEPLOY_DATA"
echo "=================="
echo ""
echo "Use this as the 'data' field in your eth_sendTransaction call."