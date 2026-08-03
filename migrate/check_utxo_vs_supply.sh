#!/usr/bin/env bash
set -euo pipefail

# Reconciles the bridge's UTXO set against the token total supply.
#
# Invariant: ft_total_supply == sum(available UTXOs)
# A non-zero diff can be transient (deposit mint or withdraw burn in flight) —
# re-run after a minute before treating it as a real discrepancy.
#
# Usage: check_utxo_vs_supply.sh

BRIDGE="zcash-connector.bridge.near"
RPC="https://rpc.mainnet.near.org"
PAGE=50

rpc_call() {
    local params="$1"
    local response
    response=$(curl -sS "$RPC" -H 'Content-Type: application/json' -d "{
        \"jsonrpc\": \"2.0\",
        \"id\": \"1\",
        \"method\": \"query\",
        \"params\": $params
    }")
    if [ "$(printf '%s' "$response" | jq 'has("error") or (.result | has("error"))')" != "false" ]; then
        echo "RPC error for params $params:" >&2
        printf '%s\n' "$response" | jq '.error // .result.error' >&2
        exit 1
    fi
    printf '%s' "$response" | jq -r '.result.result | implode'
}

view_call() {
    local account="$1" method="$2" args_json="$3"
    local args_base64
    args_base64=$(printf '%s' "$args_json" | base64 -w0)
    rpc_call "{
        \"request_type\": \"call_function\",
        \"block_id\": $BLOCK_HEIGHT,
        \"account_id\": \"$account\",
        \"method_name\": \"$method\",
        \"args_base64\": \"$args_base64\"
    }"
}

sum_utxos_paged() {
    local method="$1"
    local sum=0 count=0 from=0 page page_count
    while :; do
        page=$(view_call "$BRIDGE" "$method" "{\"from_index\": $from, \"limit\": $PAGE}")
        page_count=$(printf '%s' "$page" | jq 'length')
        sum=$(printf '%s' "$page" | jq "[.[].balance | tonumber] | add // 0 | . + $sum")
        count=$((count + page_count))
        echo "  $method: fetched $count UTXOs..." >&2
        from=$((from + page_count))
        [ "$page_count" -lt "$PAGE" ] && break
    done
    echo "$sum $count"
}

BLOCK_HEIGHT=$(curl -sS "$RPC" -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":"1","method":"block","params":{"finality":"final"}}' \
    | jq '.result.header.height')
echo "Pinned to final block: $BLOCK_HEIGHT" >&2

TOKEN=$(view_call "$BRIDGE" get_config '{}' | jq -r '.nbtc_account_id')
echo "Bridge: $BRIDGE, token: $TOKEN" >&2

METADATA=$(view_call "$BRIDGE" get_metadata '{}')
EXPECTED_UTXOS=$(printf '%s' "$METADATA" | jq -r '.current_utxos_num')

read -r AVAILABLE_SUM AVAILABLE_COUNT <<< "$(sum_utxos_paged get_utxos_paged)"
read -r UNAVAILABLE_SUM UNAVAILABLE_COUNT <<< "$(sum_utxos_paged get_unavailable_utxos_paged)"

TOTAL_SUPPLY=$(view_call "$TOKEN" ft_total_supply '{}' | jq -r 'tonumber')

DIFF=$((AVAILABLE_SUM - TOTAL_SUPPLY))

echo ""
echo "=== UTXO vs total supply (block $BLOCK_HEIGHT) ==="
echo "Available UTXOs:         $AVAILABLE_COUNT pcs, sum = $AVAILABLE_SUM"
echo "Unavailable UTXOs:       $UNAVAILABLE_COUNT pcs, sum = $UNAVAILABLE_SUM (informational)"
echo "ft_total_supply:         $TOTAL_SUPPLY"
echo "Diff (available-supply): $DIFF"

if [ "$AVAILABLE_COUNT" -ne "$EXPECTED_UTXOS" ]; then
    echo "WARNING: fetched $AVAILABLE_COUNT available UTXOs, but metadata reports $EXPECTED_UTXOS" >&2
fi

if [ "$DIFF" -eq 0 ]; then
    echo "OK: available UTXO sum matches total supply"
else
    echo "MISMATCH: diff = $DIFF (may be transient if a mint/burn is in flight — re-run to confirm)"
    exit 1
fi
