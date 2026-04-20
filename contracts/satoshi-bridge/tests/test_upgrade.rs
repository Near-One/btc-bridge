mod setup;
use near_sdk::serde_json::json;
use satoshi_bridge::{Account, Config};
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
async fn test_btc_bridge_upgrade_from_v0_7_5() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0-7-5.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
}

#[tokio::test]
async fn test_zcash_bridge_upgrade_from_v0_6_0() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/zcash_bridge_v0-6-0.wasm",
        "tests/data/nbtc_v0-6-0.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/zcash_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
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
        "tests/data/btc_bridge_v0-7-5.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_nbtc_version());
    check!(upgrade_context.upgrade_nbtc("../../res/nbtc.wasm"));
    check!(view upgrade_context.get_nbtc_version());
}

/// After upgrading from v0.7.5 (btc_pending_sign_id: Option<String>)
/// to the current version (btc_pending_sign_ids: HashSet<String>),
/// reading an account created by the old contract must still work.
#[tokio::test]
async fn test_btc_bridge_upgrade_from_v0_7_5_account_migration() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0-7-5.wasm",
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

    // get_config must deserialize into the new Config layout, proving the
    // V3 config migration populated `refund_timelock_sec` with the default
    // (v0.7.5 had no such field).
    let config: Config = upgrade_context
        .previous_satoshi_bridge_contract
        .call("get_config")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();

    assert_eq!(config.refund_timelock_sec, 14 * 24 * 3600);
}

#[tokio::test]
async fn test_set_icon() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, None).await;
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
    check!(context.set_metadata("new icon"));
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
}
