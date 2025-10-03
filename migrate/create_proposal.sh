EXPECTED_NBTC_BS58_HASH=HgZnctwS7JdgbnXH6sbanQ9XoZCdf7iSz9vCdvMtRrGC
NBTC_ACCOUNT_ID=nbtc.bridge.near
DAO_ACCOUNT_ID=rainbowbridge.sputnik-dao.near
SIGNER_ACCOUNT_ID=bridge-ops.near
NEAR_NETWORK=mainnet

mkdir -p tmp

cd ../contracts/nbtc
cargo near build reproducible-wasm
cd ../../migrate

NBTC_WASM_PATH=../target/near/nbtc/nbtc.wasm
ACTUAL_NBTC_BS58_HASH=$(sha256sum $NBTC_WASM_PATH | awk '{print $1}' | xxd -r -p | base58)

if [[ "$ACTUAL_NBTC_BS58_HASH" != "$EXPECTED_NBTC_BS58_HASH" ]]; then
  echo "❌ Incorrect nBTC wasm hash"
  echo "Expected: $EXPECTED_NBTC_BS58_HASH"
  echo "Actual: $ACTUAL_NBTC_BS58_HASH"
  exit 1
fi

MIGRATE_METHOD="${MIGRATE_METHOD:-upgrade_and_migrate}"
MIGRATE_ARGS_JSON="${MIGRATE_ARGS_JSON:-{}}"
MIGRATE_ARGS_B64=$(printf '%s' "$MIGRATE_ARGS_JSON" | tr -d '\n' | base64 | tr -d '\n')

near contract call-function as-transaction \
  "$DAO_ACCOUNT_ID" \
  store_blob \
  file-args "$NBTC_WASM_PATH" \
  prepaid-gas '50.0 Tgas' \
  attached-deposit '3 NEAR' \
  sign-as "$SIGNER_ACCOUNT_ID" \
  network-config "$NEAR_NETWORK" \
  sign-with-keychain send

cat > ./tmp/proposal.json <<JSON
{
   "proposal": {
   "description": "Upgrade + migrate nBTC via UpgradeRemote",
    "kind": {
      "UpgradeRemote": {
        "receiver_id": "$NBTC_ACCOUNT_ID",
        "method_name": "$MIGRATE_METHOD",
        "hash": "$ACTUAL_NBTC_BS58_HASH",
        "args": "$MIGRATE_ARGS_B64"
      }
    }
  }
}
JSON

near contract call-function as-transaction \
  "$DAO_ACCOUNT_ID" \
  add_proposal \
  file-args ./tmp/proposal.json \
  prepaid-gas '100.0 Tgas' \
  attached-deposit '1 NEAR' \
  sign-as "$SIGNER_ACCOUNT_ID" \
  network-config "$NEAR_NETWORK" \
  sign-with-keychain send

echo "✅ Proposal submitted: UpgradeRemote -> $NBTC_ACCOUNT_ID (hash=$ACTUAL_NBTC_BS58_HASH, method=$MIGRATE_METHOD)"
