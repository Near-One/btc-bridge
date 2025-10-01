PROPOSAL_ID=
EXPECTED_NBTC_BS58_HASH=HgZnctwS7JdgbnXH6sbanQ9XoZCdf7iSz9vCdvMtRrGC
DAO_ACCOUNT_ID=rainbowbridge.sputnik-dao.near

mkdir -p tmp

PROP_JSON=./tmp/actual_proposal.json
WASM_PATH=./tmp/decoded_args.wasm

near view "$DAO_ACCOUNT_ID" get_proposal "{\"id\": $PROPOSAL_ID}" > $PROP_JSON

if ! jq -e '.kind.FunctionCall.actions[0].args' "$PROP_JSON" >/dev/null 2>&1; then
  echo "❌ kind.FunctionCall.actions[0].args not found"
  echo "File: $PROP_JSON"
  exit 1
fi

WASM_B64="$(jq -r '.kind.FunctionCall.actions[0].args' "$PROP_JSON")"
printf '%s' "$WASM_B64" | base64 -d > "$WASM_PATH"

DECODED_NBTC_BS58_HASH=$(sha256sum $WASM_PATH | awk '{print $1}' | xxd -r -p | base58)
if [[ "$DECODED_NBTC_BS58_HASH" != "$EXPECTED_NBTC_BS58_HASH" ]]; then
  echo "❌ Incorrect nBTC wasm hash"
  echo "Expected: $EXPECTED_NBTC_BS58_HASH"
  echo "Actual: $DECODED_NBTC_BS58_HASH"
  exit 1
else
  echo "✅ nBTC wasm hash is correct"
  exit 0
fi
