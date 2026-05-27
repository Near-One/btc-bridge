mod setup;
#[cfg(not(feature = "zcash"))]
use near_sdk::serde_json::json;
#[cfg(not(feature = "zcash"))]
use satoshi_bridge::Account;
use satoshi_bridge::{Config, DEFAULT_REFUND_TIMELOCK_SEC, DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC};
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
async fn test_btc_bridge_upgrade_from_v0_8_0() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0-8-0.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
}

/// Upgrade from the currently deployed Zcash version (0.7.4) to the version in
/// this PR. Exercises the `refund_requests` storage migration (the field is
/// un-gated for Zcash here) and confirms the config still deserializes.
#[tokio::test]
async fn test_zcash_bridge_upgrade_from_v7_4_0() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/zcash_bridge_v7.4.0.wasm",
        "tests/data/nbtc_v0-6-0.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/zcash_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());

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
        "tests/data/btc_bridge_v0-8-0.wasm",
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
        "tests/data/btc_bridge_v0-8-0.wasm",
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

#[tokio::test]
async fn test_set_icon() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, None).await;
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
    check!(context.set_metadata("new icon"));
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
}
