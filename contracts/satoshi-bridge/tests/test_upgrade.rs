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

/// Reproduces the migration bug for `btc_pending_infos` introduced by the
/// Subsidize RBF PR.
///
/// Setup:
///   1. Deploy v0.8.4 bridge (with the old, single-variant
///      `enum VBTCPendingInfo { Current(BTCPendingInfo) }`).
///   2. Create one pending info via the real deposit→withdraw flow.
///   3. Upgrade to the current wasm, which redefines the enum as
///      `enum VBTCPendingInfo { V0(BTCPendingInfoV0), Current(BTCPendingInfo) }`.
///      The borsh discriminant 0 on existing entries now deserializes as `V0`.
///
/// Expectation after upgrade:
///   - View-path via `internal_view_btc_pending_info` (clone-based, handles
///     both V0 and Current) keeps working. — ASSERTED.
///   - Any operational path that goes through `internal_unwrap_btc_pending_info`
///     (returns `&BTCPendingInfo`, conversion hits `unreachable!()` on V0)
///     must NOT panic with "unreachable". — CURRENTLY FAILS because the
///     `Current → Current` arm of `migrate_state` does not run
///     `migrate_btc_pending_infos_to_current`.
///
/// This test should remain failing until the migration is fixed (either by
/// triggering eager pending-info migration on Current→Current upgrades, or by
/// making the immutable accessor return an owned `BTCPendingInfo`).
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_btc_bridge_upgrade_from_v0_8_4_pending_info_survives_unwrap() {
    use bitcoin::{Amount, OutPoint, TxOut};
    use satoshi_bridge::network::{Address, Chain};
    use satoshi_bridge::{PendingInfoState, TokenReceiverMessage};

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new_with_bridge_wasm(
        &worker,
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

    // Snapshot the pending info before upgrade.
    let pendings_before = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pendings_before.len(), 1, "expected exactly one pending info");
    let (pending_id, info_before) = pendings_before.into_iter().next().unwrap();

    // Upgrade to the current version (this PR).
    check!(context.upgrade_satoshi_bridge("../../res/bitcoin_bridge.wasm"));

    // View-path: must still deserialize and surface the new `subsidize_amount`
    // field defaulted to 0.
    let pendings_after = context.get_btc_pending_infos_paged().await.unwrap();
    let info_after = pendings_after
        .get(&pending_id)
        .expect("pending info must survive view-path after upgrade");
    assert_eq!(info_after.account_id, info_before.account_id);
    assert_eq!(info_after.transfer_amount, info_before.transfer_amount);
    assert_eq!(info_after.gas_fee, info_before.gas_fee);
    assert_eq!(
        info_after.actual_received_amount,
        info_before.actual_received_amount
    );
    match &info_after.state {
        PendingInfoState::WithdrawOriginal(state) => {
            assert_eq!(
                state.subsidize_amount, 0,
                "new field must default to 0 on migrated entries"
            );
        }
        other => panic!("expected WithdrawOriginal state, got {other:?}"),
    }

    // Mutating-path: `verify_withdraw` calls `internal_unwrap_btc_pending_info`,
    // which converts `&VBTCPendingInfo` → `&BTCPendingInfo`. For `V0` entries
    // that conversion currently hits `unreachable!()` (see
    // `From<&'a VBTCPendingInfo> for &'a BTCPendingInfo` in `btc_pending_info.rs`).
    //
    // We expect verify_withdraw to fail (the merkle proof is empty/fake),
    // but it MUST fail for a domain reason — not by panicking on the unwrap.
    let result = context
        .verify_withdraw(
            "relayer",
            &pending_id,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
        )
        .await
        .expect("verify_withdraw tx must be sent (execution may still fail)");

    let failures = format!("{:?}", result.receipt_failures());
    assert!(
        !failures.contains("unreachable"),
        "PR breaks read access to pending infos migrated from v0.8.4: \
         `internal_unwrap_btc_pending_info` hits `unreachable!()` on `VBTCPendingInfo::V0`.\n\
         The `Current → Current` arm of `migrate_state` does not eagerly migrate \
         `btc_pending_infos`. After upgrade, every operational read path \
         (`verify_withdraw`, `sign_btc_transaction`, `cancel_*`, RBF, refund, burn) \
         panics on existing pending entries.\n\n\
         Receipt failures: {failures}"
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
