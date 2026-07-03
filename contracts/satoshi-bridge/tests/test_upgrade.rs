mod setup;
use near_sdk::serde_json::json;
use satoshi_bridge::{
    Account, Config, RefundRequest, DEFAULT_REFUND_TIMELOCK_SEC, DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC,
};
use setup::*;

#[tokio::test]
async fn test_btc_bridge_upgrade() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "../../res/bitcoin_bridge.wasm",
        "../../res/nbtc.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
}

#[tokio::test]
async fn test_btc_bridge_upgrade_from_v0_8_4() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0.8.4.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
}

/// Upgrade from the currently deployed Zcash version (0.7.4) to this PR (0.8.4).
/// Exercises the `refund_requests` storage migration (the field is un-gated for
/// Zcash here) and confirms existing state survives.
#[tokio::test]
async fn test_zcash_bridge_upgrade_from_v7_4_0() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/zcash_bridge_v7.4.0.wasm",
        "tests/data/nbtc_v0-6-0.wasm",
    )
    .await;

    // The upgrade must actually swap the contract code (0.7.4 -> current).
    let old_version = upgrade_context.get_satoshi_bridge_version().await.unwrap();
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/zcash_bridge.wasm"));
    let new_version = upgrade_context.get_satoshi_bridge_version().await.unwrap();
    assert_ne!(
        old_version, new_version,
        "upgrade should swap the contract code"
    );

    // Config still deserializes into the new layout.
    let config: Config = upgrade_context
        .previous_satoshi_bridge_contract
        .call("get_config")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(config.refund_timelock_sec, DEFAULT_REFUND_TIMELOCK_SEC);
    assert_eq!(
        config.unsafe_refund_timelock_sec,
        DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC
    );

    // The newly un-gated refund storage is initialized and queryable (empty),
    // proving the `refund_requests` field migrated in without panicking.
    let refund_requests: std::collections::HashMap<String, near_sdk::serde_json::Value> =
        upgrade_context
            .previous_satoshi_bridge_contract
            .call("get_refund_requests_paged")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json()
            .unwrap();
    assert!(
        refund_requests.is_empty(),
        "migrated contract should start with no refund requests"
    );

    // Pre-existing accounts (the deployer created at init) survive migration —
    // adding `refund_requests` to ContractData must not corrupt other state.
    let account: Option<Account> = upgrade_context
        .previous_satoshi_bridge_contract
        .call("get_account")
        .args_json(json!({"account_id": upgrade_context.root.id()}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    let account = account.expect("root account must exist after migration");
    assert_eq!(
        account.account_id.as_str(),
        upgrade_context.root.id().as_str()
    );

    let accounts: std::collections::HashMap<near_sdk::AccountId, Account> = upgrade_context
        .previous_satoshi_bridge_contract
        .call("get_accounts_paged")
        .args_json(json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(accounts.len(), 1);
}

#[tokio::test]
async fn test_nbtc_upgrade() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "../../res/bitcoin_bridge.wasm",
        "../../res/nbtc.wasm",
    )
    .await;
    check!(view upgrade_context.get_nbtc_version());
    check!(upgrade_context.upgrade_nbtc("../../res/nbtc.wasm"));
    check!(view upgrade_context.get_nbtc_version());
}

#[tokio::test]
async fn test_nbtc_upgrade_from_v0_5_1() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0.8.4.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_nbtc_version());
    check!(upgrade_context.upgrade_nbtc("../../res/nbtc.wasm"));
    check!(view upgrade_context.get_nbtc_version());
}

/// After upgrading from v0.8.0 to the current version, accounts and config
/// stored by the old contract must still deserialize. The relevant new fields
/// added since v0.8.0: `Config::unsafe_refund_timelock_sec`.
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_btc_bridge_upgrade_from_v0_8_0_state_migration() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0.8.4.wasm",
        "../../res/nbtc.wasm",
    )
    .await;

    // The deployer account (root) was created during contract init on the old version.
    // Upgrade to the new version.
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));

    // get_account must successfully deserialize the old account format.
    let account: Option<Account> = upgrade_context
        .previous_satoshi_bridge_contract
        .call("get_account")
        .args_json(json!({"account_id": upgrade_context.root.id()}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();

    let account = account.expect("Account must exist after migration");
    assert_eq!(
        account.account_id.as_str(),
        upgrade_context.root.id().as_str()
    );
    assert!(account.btc_pending_sign_ids.is_empty());

    // get_accounts_paged must also handle V0 accounts without panicking.
    let accounts: std::collections::HashMap<near_sdk::AccountId, Account> = upgrade_context
        .previous_satoshi_bridge_contract
        .call("get_accounts_paged")
        .args_json(json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();

    assert_eq!(accounts.len(), 1);

    // get_config must deserialize into the new Config layout, proving that
    // `unsafe_refund_timelock_sec` (added after v0.8.0) is populated with the
    // default. v0.8.0 already had `refund_timelock_sec`, so it is preserved
    // from on-chain state (init used DEFAULT_REFUND_TIMELOCK_SEC).
    let config: Config = upgrade_context
        .previous_satoshi_bridge_contract
        .call("get_config")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();

    assert_eq!(config.refund_timelock_sec, DEFAULT_REFUND_TIMELOCK_SEC);
    assert_eq!(
        config.unsafe_refund_timelock_sec,
        DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC
    );
}

/// End-to-end migration of a *real* refund request stored by the deployed
/// v0.8.4 contract. v0.8.4 stores `RefundRequest` as an 8-field struct (no
/// `executed`). We create one through the real `request_refund` flow, upgrade to
/// the current version, and verify the request survives the migration with
/// `executed = false` and every original field preserved.
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_btc_bridge_upgrade_from_v0_8_4_refund_migration() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0.8.4.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;

    let bridge = &upgrade_context.previous_satoshi_bridge_contract;
    let root = &upgrade_context.root;

    let refund_btc_address = "1PAGsaT5vDz6hjzvuenSw33hWzESTR3ZHQ";
    let deposit_msg = json!({
        "recipient_id": root.id(),
        "post_actions": null,
        "extra_msg": null,
        "safe_deposit": null,
        "refund_address": refund_btc_address,
    });

    // Derive the deposit address on the OLD contract (needs the MPC pubkey, which
    // UpgradeContext::new syncs).
    let deposit_address: String = bridge
        .call("get_user_deposit_address")
        .args_json(json!({ "deposit_msg": deposit_msg }))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert!(!deposit_address.is_empty());

    // Build a BTC tx paying the deposit address and submit a refund request on
    // the OLD contract, so the 8-field `RefundRequest` is borsh-stored by v0.8.4.
    let tx_bytes = generate_transaction_bytes(
        vec![(
            "a2a5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f19",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );

    root.call(bridge.id(), "request_refund")
        .args_json(json!({
            "deposit_msg": deposit_msg,
            "refund_address": refund_btc_address,
            // v0.8.4 took `tx_bytes` as a raw byte array (Vec<u8>, not Base64VecU8)
            // and the proof fields flat (no coinbase, no nested `proof`).
            "tx_bytes": tx_bytes.clone(),
            "vout": 0u32,
            "tx_block_blockhash":
                "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d",
            "tx_index": 1,
            "merkle_proof": Vec::<String>::new(),
            "gas_fee": Option::<near_sdk::json_types::U128>::None,
        }))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // The old contract now holds exactly one (8-field) refund request.
    let before: std::collections::HashMap<String, near_sdk::serde_json::Value> = bridge
        .call("get_refund_requests_paged")
        .args_json(json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(
        before.len(),
        1,
        "old contract should store one refund request"
    );
    let key = before.keys().next().unwrap().clone();
    // The deployed v0.8.4 layout has no `executed` field.
    assert!(
        before[&key].get("executed").is_none(),
        "v0.8.4 RefundRequest must not have an `executed` field"
    );

    // Upgrade v0.8.4 -> current (runs migrate_state: V5 -> Current). Old
    // `VRefundRequest::V0` entries upgrade lazily to `Current` on read.
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));

    // The request survives and now deserializes into the 9-field layout with
    // `executed = false` and every original field preserved.
    let after: std::collections::HashMap<String, RefundRequest> = bridge
        .call("get_refund_requests_paged")
        .args_json(json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(after.len(), 1, "refund request must survive migration");
    let req = after
        .get(&key)
        .expect("refund request must survive migration under the same key");
    assert!(
        !req.executed,
        "migrated request must default `executed = false`"
    );
    assert_eq!(req.refund_address, refund_btc_address);
    assert_eq!(req.amount, 100_000);
    assert_eq!(req.vout, 0);
    assert_eq!(
        req.tx_bytes.0, tx_bytes,
        "deposit tx bytes must be preserved"
    );
    assert!(
        !req.deposit_msg_json.is_empty(),
        "deposit_msg_json must be preserved"
    );
}

#[tokio::test]
async fn test_set_icon() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, None).await;
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
    check!(context.set_metadata("new icon"));
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
}
