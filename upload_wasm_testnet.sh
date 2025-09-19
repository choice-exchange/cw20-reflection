#!/bin/bash
# We remove 'set -e' to handle errors manually and prevent silent exits.
set -uo pipefail

################################################################################
#                                 CONFIGURATION                                #
################################################################################

# -- Blockchain Details --
NODE="https://testnet.sentry.tm.injective.network:443"
CHAIN_ID="injective-888"
FEES="1500000000000000inj"
GAS="3500000"

# -- Keystore Details --
FROM="testnet"
PASSWORD="12345678"

ADMIN_ADDRESS="inj1q2m26a7jdzjyfdn545vqsude3zwwtfrdap5jgz"

# -- Contract Details --
TOKEN_WASM_FILE="./artifacts/cw20_reflection_token.wasm"
TREASURY_WASM_FILE="./artifacts/cw20_reflection_treasury.wasm"


# 1. Store the Wasm code on the blockchain.
echo "-------------------------------------------------"
echo "  1. Storing Wasm code..."
echo "-------------------------------------------------"

store_response=$(yes $PASSWORD | injectived tx wasm store "$TOKEN_WASM_FILE" \
  --from="$FROM" \
  --chain-id="$CHAIN_ID" \
  --yes --fees="$FEES" --gas="$GAS" \
  --node="$NODE")

# Check if the command succeeded by looking for the 'txhash' string.
if ! echo "$store_response" | grep -q "txhash"; then
    echo "  ❌ ERROR: Failed to submit store transaction."
    echo "  > Response from injectived:"
    echo "$store_response"
    exit 1
fi

store_txhash=$(echo "$store_response" | grep 'txhash:' | awk '{print $2}')
echo "  > Store transaction submitted: $store_txhash"

echo "  > Waiting for transaction to be indexed..."
sleep 8 # Increased wait time for more reliability

# 2. Query the transaction to get the code ID.
echo "  > Querying transaction for Code ID..."
store_query_output=$(injectived query tx "$store_txhash" --node="$NODE")

CODE_ID=$(echo "$store_query_output" | grep -A 1 'key: code_id' | grep 'value:' | head -1 | sed 's/.*value: "\(.*\)".*/\1/')

if [ -z "$CODE_ID" ]; then
    echo "  ❌ ERROR: Could not find Code ID in transaction logs for tx: $store_txhash"
    echo "  > Please check the transaction on the explorer."
    exit 1
fi
echo "  ✅ Reflection token Code stored successfully. Code ID: $CODE_ID"
echo " "



# 1. Store the Wasm code on the blockchain.
echo "-------------------------------------------------"
echo "  1. Storing Wasm code..."
echo "-------------------------------------------------"

store_response=$(yes $PASSWORD | injectived tx wasm store "$TREASURY_WASM_FILE" \
  --from="$FROM" \
  --chain-id="$CHAIN_ID" \
  --yes --fees="$FEES" --gas="$GAS" \
  --node="$NODE")

# Check if the command succeeded by looking for the 'txhash' string.
if ! echo "$store_response" | grep -q "txhash"; then
    echo "  ❌ ERROR: Failed to submit store transaction."
    echo "  > Response from injectived:"
    echo "$store_response"
    exit 1
fi

store_txhash=$(echo "$store_response" | grep 'txhash:' | awk '{print $2}')
echo "  > Store transaction submitted: $store_txhash"

echo "  > Waiting for transaction to be indexed..."
sleep 8 # Increased wait time for more reliability

# 2. Query the transaction to get the code ID.
echo "  > Querying transaction for Code ID..."
store_query_output=$(injectived query tx "$store_txhash" --node="$NODE")

CODE_ID=$(echo "$store_query_output" | grep -A 1 'key: code_id' | grep 'value:' | head -1 | sed 's/.*value: "\(.*\)".*/\1/')

if [ -z "$CODE_ID" ]; then
    echo "  ❌ ERROR: Could not find Code ID in transaction logs for tx: $store_txhash"
    echo "  > Please check the transaction on the explorer."
    exit 1
fi
echo "  ✅ Treasury Code stored successfully. Code ID: $CODE_ID"
echo " "


