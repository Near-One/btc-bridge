mod setup;
use setup::*;

#[tokio::test]
async fn test_btc_bridge_upgrade() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "../../res/satoshi_bridge.wasm",
        "../../res/nbtc.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/satoshi_bridge.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
}

#[tokio::test]
async fn test_btc_bridge_upgrade_from_v0_5_1() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "tests/data/btc_bridge_v0-5-1.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_satoshi_bridge_version());
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/satoshi_bridge.wasm"));
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
    check!(upgrade_context.upgrade_satoshi_bridge("../../res/zcash.wasm"));
    check!(view upgrade_context.get_satoshi_bridge_version());
}

#[tokio::test]
async fn test_nbtc_upgrade() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let upgrade_context = UpgradeContext::new(
        &worker,
        "../../res/satoshi_bridge.wasm",
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
        "tests/data/btc_bridge_v0-5-1.wasm",
        "tests/data/nbtc_v0-5-1.wasm",
    )
    .await;
    check!(view upgrade_context.get_nbtc_version());
    check!(upgrade_context.upgrade_nbtc("../../res/nbtc.wasm"));
    check!(view upgrade_context.get_nbtc_version());
}

#[tokio::test]
async fn test_set_icon() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
    check!(context.set_metadata("new icon"));
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
}
