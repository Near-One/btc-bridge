mod setup;
use setup::*;

#[cfg(feature = "zcash")]
use std::collections::HashMap;

/// Deposit `deposit_amount` to a fresh bridge deposit address bound to
/// `refund_address`, request a refund, and return the stored request key.
#[cfg(feature = "zcash")]
async fn deposit_and_request_refund(
    context: &Context,
    refund_address: &str,
    deposit_amount: u64,
) -> String {
    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(refund_address.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
            1,
            None,
        )],
        vec![(deposit_address.as_str(), deposit_amount)],
    );
    check!(context.request_refund(
        "relayer",
        deposit_msg,
        refund_address,
        tx_bytes,
        0,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![],
        None,
    ));

    let requests: HashMap<String, near_sdk::serde_json::Value> = context
        .bridge_contract
        .call("get_refund_requests_paged")
        .args_json(near_sdk::serde_json::json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(requests.len(), 1, "exactly one refund request expected");
    requests.keys().next().unwrap().clone()
}

/// Shielded refund happy path: a Zcash deposit that was never finalized is
/// refunded to a unified address via an Orchard bundle.
///
/// deposit (150000) ──request_refund(refund_address = UA)──▶ refund request
///   ──execute_refund(Orchard bundle, 100000)──▶ refund BTCPendingInfo
///   ──sign──▶ pending_verify ──verify_refund_finalize──▶ cleaned up
///
/// gas_fee defaults to config.max_btc_gas_fee (50000), so the Orchard output is
/// 150000 - 50000 = 100000 (a cached bundle amount).
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_shielded_to_unified_address() {
    use satoshi_bridge::zcash_utils::types::ChainSpecificData;

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_amount: u64 = 150_000;
    let gas_fee: u64 = 50_000; // config.max_btc_gas_fee default
    let refund_amount: u64 = deposit_amount - gas_fee; // 100000, cached bundle

    // Unified address (Orchard + P2PKH receivers) + Orchard bundle paying `refund_amount`.
    let (recipient_ua, bundle_hex) = setup::orchard::get_or_gen_bundle(refund_amount);

    // Deposit message with the unified address as the pre-authorized refund target.
    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(recipient_ua.clone()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    // A Zcash transaction that funds the bridge-controlled deposit address.
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
            1,
            None,
        )],
        vec![(deposit_address.as_str(), deposit_amount)],
    );
    let vout: u32 = 0;

    // The deposit is not known to the bridge yet.
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 0);

    // Request the refund (relayer is whitelisted; gas_fee = None → max_btc_gas_fee).
    check!(
        print "request_refund"
        context.request_refund(
            "relayer",
            deposit_msg.clone(),
            &recipient_ua,
            tx_bytes.clone(),
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
            None,
        )
    );

    // Fetch the stored request key (= "{tx_id}@{vout}").
    let requests: HashMap<String, near_sdk::serde_json::Value> = context
        .bridge_contract
        .call("get_refund_requests_paged")
        .args_json(near_sdk::serde_json::json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(requests.len(), 1, "exactly one refund request expected");
    let key = requests.keys().next().unwrap().clone();

    // Execute the refund from a DAO account (pre-authorized refund address →
    // timelock bypassed), supplying the Orchard bundle for the shielded payout.
    check!(
        print "execute_refund"
        context.execute_refund(
            "root",
            &key,
            Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
                expiry_height: 10000,
            }),
        )
    );

    // A refund BTCPendingInfo now exists in pending_sign.
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending_infos.len(), 1);
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    let pending_values = pending_infos.values().cloned().collect::<Vec<_>>();
    pending_values[0].assert_pending_sign();

    // Sign the single (transparent) input.
    check!(
        print "sign_btc_transaction"
        context.sign_btc_transaction("alice", &pending_keys[0], 0, 0)
    );

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_values = pending_infos.values().cloned().collect::<Vec<_>>();
    pending_values[0].assert_pending_verify();

    // Finalize: prove the refund tx was included.
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    check!(
        print "verify_refund_finalize"
        context.verify_refund_finalize(
            "relayer",
            &pending_keys[0],
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
        )
    );

    // Pending info cleaned up, no nBTC minted.
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

/// Negative: the Orchard bundle pays a different amount than `deposit - gas`.
/// `validate_orchard_bundle` accepts it (recipient + internal balance are fine),
/// so the explicit refund-amount check must reject it.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_orchard_amount_mismatch() {
    use satoshi_bridge::zcash_utils::types::ChainSpecificData;

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    // deposit 150000, gas 50000 → refund_amount should be 100000.
    // The supplied bundle pays 170000 to the same recipient (so only the amount is wrong).
    let (recipient_ua, bundle_hex) = setup::orchard::get_or_gen_bundle(170_000);
    let key = deposit_and_request_refund(&context, &recipient_ua, 150_000).await;

    check!(
        context.execute_refund(
            "root",
            &key,
            Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
                expiry_height: 10000,
            }),
        ),
        "does not match refund amount"
    );

    // Nothing was created.
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
}

/// Negative: the Orchard bundle pays the correct amount but to a different
/// unified address than the stored `refund_address`.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_orchard_wrong_recipient() {
    use satoshi_bridge::zcash_utils::types::ChainSpecificData;

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    // deposit 220000, gas 50000 → refund_amount 170000.
    // Bundle A (custom key) pays 170000 to UA-A; refund_address is the default UA-B.
    let (_ua_a, bundle_a_hex) = setup::orchard::gen_bundle_with_key(170_000, [1u8; 32]);
    let (ua_b, _bundle_b_hex) = setup::orchard::get_or_gen_bundle(170_000);
    assert_ne!(_ua_a, ua_b, "test requires two distinct unified addresses");

    let key = deposit_and_request_refund(&context, &ua_b, 220_000).await;

    check!(
        context.execute_refund(
            "root",
            &key,
            Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_a_hex).unwrap().into(),
                expiry_height: 10000,
            }),
        ),
        "ERR_ORCHARD_VALIDATION"
    );

    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
}

/// Transparent refund: no Orchard bundle, funds returned to a t-address.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_transparent() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    // A Zcash testnet transparent address.
    let refund_taddr = "tmD67UTsZ4iBbhCae4D43k1x8fhFNhwd4Jn";
    let key = deposit_and_request_refund(&context, refund_taddr, 150_000).await;

    // chain_specific_data = None → transparent refund to the t-address.
    check!(
        print "execute_refund (transparent)"
        context.execute_refund("root", &key, None)
    );

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending_infos.len(), 1);
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    let pending_values = pending_infos.values().cloned().collect::<Vec<_>>();
    pending_values[0].assert_pending_sign();

    check!(context.sign_btc_transaction("alice", &pending_keys[0], 0, 0));

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    let pending_values = pending_infos.values().cloned().collect::<Vec<_>>();
    pending_values[0].assert_pending_verify();

    check!(context.verify_refund_finalize(
        "relayer",
        &pending_keys[0],
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![],
    ));

    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}
