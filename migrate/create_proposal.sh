EXPECTED_NBTC_BS58_HASH=HgZnctwS7JdgbnXH6sbanQ9XoZCdf7iSz9vCdvMtRrGC
NBTC_ACCOUNT_ID=nbtc.bridge.near
DAO_ACCOUNT_ID=rainbowbridge.sputnik-dao.near
SIGNER_ACCOUNT_ID=bridge-ops.near
NEAR_NETWORK=mainnet

cd ../contracts/nbtc
cargo near build reproducible-wasm
cd ../../migrate

NBTC_WASM_PATH=../target/near/nbtc/nbtc.wasm
ACTUAL_NBTC_BS58_HASH=$(sha256sum $NBTC_WASM_PATH | awk '{print $1}' | xxd -r -p | base58)

if [[ "ACTUAL_NBTC_BS58_HASH" != "EXPECTED_NBTC_BS58_HASH" ]]; then
  echo "❌ Incorrect nBTC wasm hash"
  echo "Expected: $EXPECTED_NBTC_BS58_HASH"
  echo "Actual: $ACTUAL_NBTC_BS58_HASH"
  exit 1
fi

WASM_B64=$(base64 -w 0 $NBTC_WASM_PATH 2>/dev/null || base64 $NBTC_WASM_PATH | tr -d '\n')

{
  echo '{
    "proposal": {
      "description": "Upgrade + migrate nBTC",
      "kind": {
        "FunctionCall": {
          "receiver_id": "'$NBTC_ACCOUNT_ID'",
          "actions": [
            {
              "method_name": "upgrade_and_migrate",
              "args": "'$WASM_B64'",
              "deposit": "0",
              "gas": "180000000000000"
            }
          ]
        }
      }
    }
  }'
} > proposal.json


near contract call-function as-transaction $DAO_ACCOUNT_ID add_proposal file-args ./proposal.json prepaid-gas '100.0 Tgas' attached-deposit '1 NEAR' sign-as $SIGNER_ACCOUNT_ID network-config $NEAR_NETWORK sign-with-keychain send
