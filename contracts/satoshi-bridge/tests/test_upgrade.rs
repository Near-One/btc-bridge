mod setup;
use near_sdk::serde_json::json;
use satoshi_bridge::{
    Account, Config, DEFAULT_REFUND_TIMELOCK_SEC, DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC,
};
use setup::*;

#[cfg(not(feature = "zcash"))]
const TARGET_ADDRESS: &str = "1PAGsaT5vDz6hjzvuenSw33hWzESTR3ZHQ";

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

/// Set up a context on v0.8.4 with exactly one pending withdraw info, return
/// the context and the pending tx id. Used by upgrade-migration tests.
#[cfg(not(feature = "zcash"))]
async fn setup_v0_8_4_with_one_pending(
    worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
) -> (Context, String) {
    use bitcoin::{Amount, OutPoint, TxOut};
    use satoshi_bridge::network::{Address, Chain};
    use satoshi_bridge::TokenReceiverMessage;

    let context = Context::new_with_bridge_wasm(
        worker,
        Some("BitcoinMainnet".to_string()),
        "tests/data/btc_bridge_v0-8-4.wasm",
    )
    .await;

    // Give alice 250000 sats of nBTC via a deposit.
    let alice_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: None,
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        })
        .await
        .unwrap();

    check!(context.verify_deposit(
        "relayer",
        DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: None,
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        },
        generate_transaction_bytes(
            vec![(
                "a2a5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f19",
                1,
                None,
            )],
            vec![(alice_btc_deposit_address.as_str(), 250000)],
        ),
        0,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![],
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 250000);

    check!(context.storage_deposit("nbtc", "bridge"));

    // Create a pending info by withdrawing 110000 sats.
    let withdraw_change_address = context.get_change_address().await.unwrap();
    let utxos = context.get_utxos_paged().await.unwrap();
    let utxo_key = utxos.keys().next().unwrap().clone();
    let utxo_parts: Vec<&str> = utxo_key.split('@').collect();

    let withdraw_amount: u128 = 110000;
    let btc_gas_fee: u64 = 25000;
    let withdraw_fee: u64 = 50000; // fee_min=50000, fee_rate=0
    let recipient_value: u64 = withdraw_amount as u64 - btc_gas_fee - withdraw_fee;
    let change_value: u64 = 250000 - (withdraw_amount as u64 - withdraw_fee);

    check!(context.do_withdraw(
        "alice",
        "bridge",
        withdraw_amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: TARGET_ADDRESS.to_string(),
            input: vec![OutPoint {
                txid: utxo_parts[0].parse().unwrap(),
                vout: utxo_parts[1].parse().unwrap(),
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(recipient_value),
                    script_pubkey: Address::parse(TARGET_ADDRESS, Chain::BitcoinMainnet)
                        .expect("Invalid btc address")
                        .script_pubkey()
                        .expect("Failed to get script pubkey"),
                },
                TxOut {
                    value: Amount::from_sat(change_value),
                    script_pubkey: Address::parse(
                        withdraw_change_address.as_str(),
                        Chain::BitcoinMainnet,
                    )
                    .expect("Invalid btc address")
                    .script_pubkey()
                    .expect("Failed to get script pubkey"),
                },
            ],
            max_gas_fee: None,
            chain_specific_data: None,
        }
    ));

    // Capture the pending id. The OLD contract's JSON response does not include
    // `subsidize_amount`, so we read raw JSON to avoid client-side schema mismatch.
    let pendings_raw: std::collections::HashMap<String, near_sdk::serde_json::Value> = context
        .bridge_contract
        .call("get_btc_pending_infos_paged")
        .args_json(json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(pendings_raw.len(), 1, "expected exactly one pending info");
    let pending_id = pendings_raw.keys().next().unwrap().clone();
    (context, pending_id)
}

/// Assert that `verify_withdraw` on the given pending id does not panic in
/// `internal_unwrap_btc_pending_info`. It can still fail for unrelated reasons
/// (the merkle proof is fake), but must not hit the `unreachable!()` branch in
/// `From<&'a VBTCPendingInfo> for &'a BTCPendingInfo` on a `V0` entry.
#[cfg(not(feature = "zcash"))]
async fn assert_verify_withdraw_does_not_hit_unreachable(context: &Context, pending_id: &str) {
    let result = context
        .verify_withdraw(
            "relayer",
            pending_id,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
        )
        .await
        .expect("verify_withdraw tx must be sent (execution may still fail)");

    let failures = format!("{:?}", result.receipt_failures());
    assert!(
        !failures.contains("unreachable"),
        "read access to a pending info hit `unreachable!()` — `migrate_state` \
         did not migrate `VBTCPendingInfo::V0` entries to `Current`.\n\n\
         Receipt failures: {failures}"
    );
}

/// Verify that on upgrade from v0.8.4, existing pending withdraw infos survive
/// both the view-path and any operational path that goes through
/// `internal_unwrap_btc_pending_info`. Regression for the migration bug where
/// `migrate_state`'s `Current → Current` arm did not eagerly migrate
/// `btc_pending_infos`, leaving all entries as `VBTCPendingInfo::V0` and
/// blocking every in-flight withdraw after upgrade.
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_btc_bridge_upgrade_from_v0_8_4_pending_info_survives_unwrap() {
    use satoshi_bridge::PendingInfoState;

    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, pending_id) = setup_v0_8_4_with_one_pending(&worker).await;

    check!(context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));

    // View-path: must surface the new `subsidize_amount` field defaulted to 0.
    let pendings_after = context.get_btc_pending_infos_paged().await.unwrap();
    let info_after = pendings_after
        .get(&pending_id)
        .expect("pending info must survive view-path after upgrade");
    match &info_after.state {
        PendingInfoState::WithdrawOriginal(state) => {
            assert_eq!(
                state.subsidize_amount, 0,
                "new field must default to 0 on migrated entries"
            );
        }
        other => panic!("expected WithdrawOriginal state, got {other:?}"),
    }

    assert_verify_withdraw_does_not_hit_unreachable(&context, &pending_id).await;
}

/// Upgrading twice in a row must be safe: the second `migrate_state` runs on
/// already-`Current` `VBTCPendingInfo` entries and must leave the data
/// unchanged (no panic, `subsidize_amount` stays at 0).
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_btc_bridge_upgrade_from_v0_8_4_double_migration_is_idempotent() {
    use satoshi_bridge::PendingInfoState;

    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, pending_id) = setup_v0_8_4_with_one_pending(&worker).await;

    check!(context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));
    check!(context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));

    let pendings_after = context.get_btc_pending_infos_paged().await.unwrap();
    let info_after = pendings_after
        .get(&pending_id)
        .expect("pending info must survive two upgrades");
    match &info_after.state {
        PendingInfoState::WithdrawOriginal(state) => {
            assert_eq!(
                state.subsidize_amount, 0,
                "double migration must keep `subsidize_amount` at 0"
            );
        }
        other => panic!("expected WithdrawOriginal state, got {other:?}"),
    }

    assert_verify_withdraw_does_not_hit_unreachable(&context, &pending_id).await;
}

#[tokio::test]
async fn test_set_icon() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, None).await;
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
    check!(context.set_metadata("new icon"));
    println!("{:?}", context.ft_metadata().await.unwrap().icon);
}
