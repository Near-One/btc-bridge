mod setup;
use setup::*;

#[cfg(feature = "zcash")]
use near_sdk::serde_json::json;
#[cfg(feature = "zcash")]
use std::collections::HashMap;

/// A Zcash testnet transparent address used as the refund target for scenarios
/// that exercise request/reject/race/timelock logic (not the shielded payout).
#[cfg(feature = "zcash")]
const ZEC_REFUND_TADDR: &str = "tmD67UTsZ4iBbhCae4D43k1x8fhFNhwd4Jn";

#[cfg(feature = "zcash")]
const BLOCKHASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";

/// Fetch the key of the single stored refund request (Zcash tx ids are computed
/// differently from Bitcoin, so we read it back rather than recompute locally).
#[cfg(feature = "zcash")]
async fn refund_key(context: &Context) -> String {
    let requests: HashMap<String, near_sdk::serde_json::Value> = context
        .bridge_contract
        .call("get_refund_requests_paged")
        .args_json(json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    requests
        .keys()
        .next()
        .expect("a refund request to exist")
        .clone()
}

/// Set `refund_timelock_sec` (root/DAO only).
#[cfg(feature = "zcash")]
async fn set_refund_timelock(context: &Context, secs: u64) {
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": secs}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();
}

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

/// DAO rejects a refund request; execution afterwards fails.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_reject() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let key = deposit_and_request_refund(&context, ZEC_REFUND_TADDR, 100_000).await;

    check!(print "reject_refund" context.reject_refund("root", &key));

    check!(
        context.execute_refund("alice", &key, None),
        "Refund request not found"
    );
}

/// request_refund succeeds even when deposit_msg.refund_address is None
/// (address supplied as a separate parameter).
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_no_refund_address() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: None,
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "c4c5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f21",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );

    check!(
        print "request_refund_no_addr_in_msg"
        context.request_refund(
            "alice", deposit_msg, ZEC_REFUND_TADDR, tx_bytes, 0,
            BLOCKHASH.to_string(), 1, vec![], None,
        )
    );
}

/// A second request for the same UTXO is rejected.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_duplicate_request() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "d5d5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f22",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );

    check!(
        print "first request"
        context.request_refund(
            "alice", deposit_msg.clone(), ZEC_REFUND_TADDR, tx_bytes.clone(), 0,
            BLOCKHASH.to_string(), 1, vec![], None,
        )
    );
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            ZEC_REFUND_TADDR,
            tx_bytes,
            0,
            BLOCKHASH.to_string(),
            1,
            vec![],
            None,
        ),
        "Refund request already exists for this UTXO"
    );
}

/// After execute_refund, verify_deposit is permanently blocked for that UTXO.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_then_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "e6e6069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f23",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.request_refund(
        "relayer",
        deposit_msg.clone(),
        ZEC_REFUND_TADDR,
        tx_bytes.clone(),
        vout,
        BLOCKHASH.to_string(),
        1,
        vec![],
        None,
    ));

    set_refund_timelock(&context, 0).await;
    let key = refund_key(&context).await;

    // Transparent refund (no Orchard bundle).
    check!(print "execute_refund" context.execute_refund("alice", &key, None));

    check!(
        context.verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    let pending_keys = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(context.sign_btc_transaction("alice", &pending_keys[0], 0, 0));

    check!(
        context.verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    check!(context.verify_refund_finalize(
        "relayer",
        &pending_keys[0],
        BLOCKHASH.to_string(),
        1,
        vec![]
    ));

    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    check!(
        context.verify_deposit(
            "relayer",
            deposit_msg,
            tx_bytes,
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

/// During the timelock, a deposit wins the race; execute_refund then fails.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_race_deposit_wins() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "f7f7069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f24",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.request_refund(
        "relayer",
        deposit_msg.clone(),
        ZEC_REFUND_TADDR,
        tx_bytes.clone(),
        vout,
        BLOCKHASH.to_string(),
        1,
        vec![],
        None,
    ));
    let key = refund_key(&context).await;

    check!(print "verify_deposit" context.verify_deposit(
        "relayer", deposit_msg, tx_bytes, vout, BLOCKHASH.to_string(), 1, vec![]
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);

    set_refund_timelock(&context, 0).await;
    check!(
        context.execute_refund("alice", &key, None),
        "UTXO already verified via deposit, cannot refund"
    );
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);
}

/// A finalized deposit blocks a later refund request.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_after_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "a8a8069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f25",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(print "verify_deposit" context.verify_deposit(
        "relayer", deposit_msg.clone(), tx_bytes.clone(), vout, BLOCKHASH.to_string(), 1, vec![]
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);

    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            ZEC_REFUND_TADDR,
            tx_bytes,
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![],
            None
        ),
        "UTXO already verified via deposit"
    );
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);
}

/// After a rejection the UTXO is untouched, so a normal deposit still works.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_reject_then_deposit_succeeds() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "b9b9069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f26",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.request_refund(
        "relayer",
        deposit_msg.clone(),
        ZEC_REFUND_TADDR,
        tx_bytes.clone(),
        vout,
        BLOCKHASH.to_string(),
        1,
        vec![],
        None,
    ));
    let key = refund_key(&context).await;

    check!(print "reject_refund" context.reject_refund("root", &key));
    check!(
        context.execute_refund("alice", &key, None),
        "Refund request not found"
    );

    check!(print "verify_deposit" context.verify_deposit(
        "relayer", deposit_msg, tx_bytes, vout, BLOCKHASH.to_string(), 1, vec![]
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);
}

/// After execute_refund, a second request for the same UTXO is rejected.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_double_request_after_execute() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "caca069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f27",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.request_refund(
        "relayer",
        deposit_msg.clone(),
        ZEC_REFUND_TADDR,
        tx_bytes.clone(),
        vout,
        BLOCKHASH.to_string(),
        1,
        vec![],
        None,
    ));
    set_refund_timelock(&context, 0).await;
    let key = refund_key(&context).await;

    check!(print "execute_refund" context.execute_refund("alice", &key, None));

    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            ZEC_REFUND_TADDR,
            tx_bytes,
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![],
            None
        ),
        "UTXO already verified via deposit"
    );
}

/// A request whose deposit_msg.refund_address disagrees with the provided
/// refund_address is rejected (anti-spoofing).
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_spoofed_refund_address() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let real_deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(real_deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "dbdb069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f28",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    let spoofed_deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some("tmEgW8c44RQQfft9FHXnqGp8XEcQQSRcUXD".to_string()),
    };

    check!(
        context.request_refund(
            "bob",
            spoofed_deposit_msg,
            ZEC_REFUND_TADDR,
            tx_bytes.clone(),
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![],
            None
        ),
        "refund_address does not match deposit_msg.refund_address"
    );

    check!(print "real request_refund" context.request_refund(
        "alice", real_deposit_msg, ZEC_REFUND_TADDR, tx_bytes, vout, BLOCKHASH.to_string(), 1, vec![], None
    ));
}

/// During the timelock, a safe deposit wins; execute_refund then fails.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_race_safe_deposit_wins() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: Some(satoshi_bridge::SafeDepositMsg {
            msg: "".to_string(),
        }),
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "ecec069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f29",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.request_refund(
        "relayer",
        deposit_msg.clone(),
        ZEC_REFUND_TADDR,
        tx_bytes.clone(),
        vout,
        BLOCKHASH.to_string(),
        1,
        vec![],
        None,
    ));
    let key = refund_key(&context).await;

    check!(context.storage_deposit("nbtc", "alice"));
    check!(print "safe_verify_deposit" context.safe_verify_deposit(
        "relayer", deposit_msg, tx_bytes, vout, BLOCKHASH.to_string(), 1, vec![]
    ));
    assert!(context.ft_balance_of("alice").await.unwrap().0 > 0);

    set_refund_timelock(&context, 0).await;
    check!(
        context.execute_refund("alice", &key, None),
        "UTXO already verified via deposit, cannot refund"
    );
}

/// A finalized safe deposit blocks a later refund request.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_after_safe_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: Some(satoshi_bridge::SafeDepositMsg {
            msg: "".to_string(),
        }),
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "fdfd069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f30",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.storage_deposit("nbtc", "alice"));
    check!(print "safe_verify_deposit" context.safe_verify_deposit(
        "relayer", deposit_msg.clone(), tx_bytes.clone(), vout, BLOCKHASH.to_string(), 1, vec![]
    ));
    assert!(context.ft_balance_of("alice").await.unwrap().0 > 0);

    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            ZEC_REFUND_TADDR,
            tx_bytes,
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![],
            None
        ),
        "UTXO already verified via deposit"
    );
}

/// After execute_refund, safe_verify_deposit is permanently blocked.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_then_safe_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: Some(satoshi_bridge::SafeDepositMsg {
            msg: "".to_string(),
        }),
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "abab069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f31",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.storage_deposit("nbtc", "alice"));
    check!(context.request_refund(
        "relayer",
        deposit_msg.clone(),
        ZEC_REFUND_TADDR,
        tx_bytes.clone(),
        vout,
        BLOCKHASH.to_string(),
        1,
        vec![],
        None,
    ));
    set_refund_timelock(&context, 0).await;
    let key = refund_key(&context).await;

    check!(print "execute_refund" context.execute_refund("alice", &key, None));

    check!(
        context.safe_verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    let pending_keys = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(context.sign_btc_transaction("alice", &pending_keys[0], 0, 0));

    check!(
        context.safe_verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    check!(context.verify_refund_finalize(
        "relayer",
        &pending_keys[0],
        BLOCKHASH.to_string(),
        1,
        vec![]
    ));
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    check!(
        context.safe_verify_deposit(
            "relayer",
            deposit_msg,
            tx_bytes,
            vout,
            BLOCKHASH.to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

/// request_refund succeeds when refund_address matches deposit_msg.refund_address.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_address_matches_deposit_msg() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "e1e1069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f30",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );

    check!(print "matching" context.request_refund(
        "alice", deposit_msg, ZEC_REFUND_TADDR, tx_bytes, 0, BLOCKHASH.to_string(), 1, vec![], None
    ));
}

/// request_refund succeeds when deposit_msg.refund_address is None.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_address_none_in_deposit_msg() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: None,
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "f2f2069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f31",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );

    check!(print "external addr" context.request_refund(
        "alice", deposit_msg, ZEC_REFUND_TADDR, tx_bytes, 0, BLOCKHASH.to_string(), 1, vec![], None
    ));
}

/// request_refund fails when refund_address disagrees with deposit_msg.refund_address.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_address_mismatch() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "a3a3069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f32",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );

    let wrong_address = "tmEgW8c44RQQfft9FHXnqGp8XEcQQSRcUXD";
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            wrong_address,
            tx_bytes,
            0,
            BLOCKHASH.to_string(),
            1,
            vec![],
            None
        ),
        "refund_address does not match deposit_msg.refund_address"
    );
}

/// Operator/DAO can execute a refund before the timelock elapses.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_operator_skips_timelock() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(ZEC_REFUND_TADDR.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(
            "f4f5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f30",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );

    check!(context.request_refund(
        "alice",
        deposit_msg.clone(),
        ZEC_REFUND_TADDR,
        tx_bytes,
        0,
        BLOCKHASH.to_string(),
        1,
        vec![],
        None,
    ));
    let key = refund_key(&context).await;

    // Long timelock so a regular user is blocked.
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 999999, "unsafe_refund_timelock_sec": 999999}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    check!(
        context.execute_refund("alice", &key, None),
        "Refund timelock has not passed yet"
    );

    check!(context.bridge_acl_grant_role(
        "root",
        "Operator",
        &context.get_account_by_name("alice").sdk_id()
    ));

    check!(print "execute as operator" context.execute_refund("alice", &key, None));
}

/// Negative: a shielded refund whose requested expiry height is beyond the
/// ~3-month window is rejected.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_zcash_refund_orchard_expiry_too_far() {
    use satoshi_bridge::zcash_utils::types::ChainSpecificData;

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    // deposit 150000, gas 50000 → refund 100000 (cached bundle).
    let (recipient_ua, bundle_hex) = setup::orchard::get_or_gen_bundle(100_000);
    let key = deposit_and_request_refund(&context, &recipient_ua, 150_000).await;

    // Mock light client height is 1000, so max allowed expiry = 1000 + 103_680.
    check!(
        context.execute_refund(
            "root",
            &key,
            Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
                expiry_height: 1_000_000, // far beyond the 3-month window
            }),
        ),
        "Invalid refund expiry height"
    );

    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
}
