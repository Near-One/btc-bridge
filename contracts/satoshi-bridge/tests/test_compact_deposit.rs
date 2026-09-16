//! `verify_deposit_v2` driven by a ZIP-244 compact proof passed as `tx_bytes`,
//! rather than the full transaction bytes.

mod setup;
use setup::*;

#[cfg(feature = "zcash")]
const BLOCK_HASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";

#[cfg(feature = "zcash")]
const PREV_TX_ID: &str = "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234";

#[cfg(feature = "zcash")]
fn alice_deposit_msg(context: &Context) -> DepositMsg {
    DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: None,
    }
}

#[cfg(feature = "zcash")]
fn real_tx_id(tx_bytes: &[u8]) -> String {
    satoshi_bridge::WrappedTransaction::decode(
        tx_bytes,
        &satoshi_bridge::network::Chain::ZcashTestnet,
    )
    .unwrap()
    .compute_txid()
    .to_string()
}

/// A Zcash sandbox plus a transaction with `inputs` transparent inputs, paying
/// `amount` to alice's deposit address at output index 0.
#[cfg(feature = "zcash")]
async fn setup_deposit(
    worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
    amount: u64,
    inputs: u32,
) -> (Context, Vec<u8>) {
    std::env::set_var("TEST_CHAIN", "ZcashTestnet");
    let context = Context::new(worker, None).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));

    let alice_deposit_address = context
        .get_user_deposit_address(alice_deposit_msg(&context))
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        (0..inputs)
            .map(|vout| (PREV_TX_ID, vout, Some("1MgiBKohM2poApYamQadp21vJrNyh5T19G")))
            .collect(),
        vec![(alice_deposit_address.as_str(), amount)],
    );
    (context, tx_bytes)
}

/// 40 inputs, because the compact form is a fixed ~480 bytes of JSON and only
/// pays off above roughly 500 bytes of transaction.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_compact_deposit_credits_same_utxo() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, tx_bytes) = setup_deposit(&worker, 500000, 40).await;

    let compact = setup::utils::compact_proof_json(&tx_bytes);
    let compact_len = compact.to_string().len();
    println!(
        "==> tx_bytes: {} bytes vs compact proof JSON: {compact_len} bytes",
        tx_bytes.len(),
    );
    assert!(
        compact_len < tx_bytes.len(),
        "compact proof ({compact_len}) should beat {} raw bytes at this width",
        tx_bytes.len()
    );

    check!(
        print "verify_deposit_v2 (compact)"
        context.verify_deposit_v2_compact(
            "relayer",
            alice_deposit_msg(&context),
            compact,
            0,
            proof_json(BLOCK_HASH.to_string(), 1, vec![]),
        )
    );

    // The UTXO key must match what decoding the full transaction yields.
    let expected_key = format!("{}@0", real_tx_id(&tx_bytes));
    let utxos = context.get_utxos_paged().await.unwrap();
    assert!(
        utxos.contains_key(&expected_key),
        "expected UTXO {expected_key}, got {:?}",
        utxos.keys().collect::<Vec<_>>()
    );
    assert_eq!(utxos[&expected_key].balance, 500000);
}

/// `tx_bytes` accepts both wire forms under the one parameter name, and both
/// resolve to the same txid. The base64 string is the pre-existing format and
/// must keep working verbatim.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_tx_bytes_accepts_both_wire_forms() {
    use near_sdk::json_types::Base64VecU8;
    use near_sdk::serde_json::json;

    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, tx_bytes) = setup_deposit(&worker, 500000, 1).await;
    let expected_key = format!("{}@0", real_tx_id(&tx_bytes));

    // Legacy form: a JSON string.
    check!(
        print "verify_deposit_v2 (tx_bytes as base64 string)"
        context.verify_deposit_v2_raw(
            "relayer",
            json!({
                "deposit_msg": alice_deposit_msg(&context),
                "tx_bytes": Base64VecU8(tx_bytes.clone()),
                "vout": 0,
                "proof": proof_json(BLOCK_HASH.to_string(), 1, vec![]),
            }),
        )
    );
    let utxos = context.get_utxos_paged().await.unwrap();
    assert!(
        utxos.contains_key(&expected_key),
        "string form should credit {expected_key}"
    );

    // New form: a JSON object in the same field. Re-depositing the same UTXO is
    // rejected only if the compact proof resolved to the identical txid.
    check!(
        context.verify_deposit_v2_compact(
            "relayer",
            alice_deposit_msg(&context),
            setup::utils::compact_proof_json(&tx_bytes),
            0,
            proof_json(BLOCK_HASH.to_string(), 1, vec![]),
        ),
        "Already deposit utxo"
    );
}

/// A malformed object is rejected at deserialization, not silently treated as
/// the other variant.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_tx_bytes_rejects_malformed_compact_proof() {
    use near_sdk::serde_json::json;

    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, tx_bytes) = setup_deposit(&worker, 500000, 1).await;

    // Drop a required digest field.
    let mut compact = setup::utils::compact_proof_json(&tx_bytes);
    compact.as_object_mut().unwrap().remove("orchard_digest");

    check!(
        context.verify_deposit_v2_raw(
            "relayer",
            json!({
                "deposit_msg": alice_deposit_msg(&context),
                "tx_bytes": compact,
                "vout": 0,
                "proof": proof_json(BLOCK_HASH.to_string(), 1, vec![]),
            }),
        ),
        "Failed to deserialize input from JSON"
    );
}

/// Tampering with a digest changes the recomputed txid. The mock light client
/// confirms any txid, so this asserts on the txid itself; in production the
/// inclusion check is what rejects it.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_compact_deposit_binds_digests_into_txid() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, tx_bytes) = setup_deposit(&worker, 500000, 1).await;

    let mut compact = setup::utils::compact_proof_json(&tx_bytes);
    compact["prevouts_digest"] =
        near_sdk::serde_json::to_value(near_sdk::json_types::Base64VecU8(vec![0xaa; 32])).unwrap();

    check!(
        print "verify_deposit_v2 (tampered prevouts_digest)"
        context.verify_deposit_v2_compact(
            "relayer",
            alice_deposit_msg(&context),
            compact,
            0,
            proof_json(BLOCK_HASH.to_string(), 1, vec![]),
        )
    );

    let genuine_key = format!("{}@0", real_tx_id(&tx_bytes));
    let utxos = context.get_utxos_paged().await.unwrap();
    assert!(
        !utxos.contains_key(&genuine_key),
        "a tampered prevouts_digest must not resolve to the genuine txid"
    );
}
