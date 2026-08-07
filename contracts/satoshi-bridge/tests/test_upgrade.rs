mod setup;
use near_sdk::serde_json::json;
#[cfg(not(feature = "zcash"))]
use satoshi_bridge::RefundRequest;
use satoshi_bridge::{
    Account, Config, DEFAULT_REFUND_TIMELOCK_SEC, DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC,
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
async fn test_btc_bridge_upgrade_from_v0_9_2() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0.9.2.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
}

#[tokio::test]
async fn test_zcash_bridge_upgrade_from_v0_9_5() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/zcash_bridge_v0.9.5.wasm",
        "tests/data/nbtc_v0-6-0.wasm",
    )
    .await;

    let old_version = upgrade_context.get_satoshi_bridge_version().await.unwrap();
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/zcash_bridge.wasm"));
    let new_version = upgrade_context.get_satoshi_bridge_version().await.unwrap();
    assert_ne!(
        old_version, new_version,
        "upgrade should swap the contract code"
    );

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
        "tests/data/btc_bridge_v0.9.2.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_nbtc_version());
    check!(upgrade_context.upgrade_nbtc("../../res/nbtc.wasm"));
    check!(view upgrade_context.get_nbtc_version());
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_btc_bridge_upgrade_from_v0_9_2_state_migration() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0.9.2.wasm",
        "../../res/nbtc.wasm",
    )
    .await;

    check!(upgrade_context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));

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
