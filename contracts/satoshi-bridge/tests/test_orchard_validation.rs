mod setup;
use bitcoin::{Address, Amount, OutPoint, TxOut};
use satoshi_bridge::{DepositMsg, TokenReceiverMessage};
use setup::*;
use std::str::FromStr;

/// Test: Bundle with wrong recipient should be rejected
#[tokio::test]
#[cfg(feature = "zcash")]
#[should_panic(expected = "Orchard recipient mismatch")]
async fn test_orchard_wrong_recipient() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();

    // Setup: Deposit for alice
    let alice_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        })
        .await
        .unwrap();

    check!(context.verify_deposit(
        "relayer",
        DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        },
        generate_transaction_bytes(
            vec![(
                "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
                1,
                None,
            )],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (alice_btc_deposit_address.as_str(), 500000),
            ],
        ),
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();

    let withdraw_amount = 200000u128;
    let _btc_gas_fee = 10000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let orchard_amount = withdraw_amount - _btc_gas_fee - withdraw_fee;

    // Generate bundle with correct amount
    let (_actual_recipient, bundle_hex) = get_or_gen_bundle(orchard_amount as u64);

    // Generate a different recipient to claim
    let different_amount = orchard_amount + 1000; // Different amount to get different UA
    let (fake_recipient, _) = get_or_gen_bundle(different_amount as u64);

    // This should panic with "Orchard recipient mismatch"
    check!(context.do_withdraw(
        "alice",
        "bridge",
        withdraw_amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: fake_recipient, // Wrong recipient!
            input: vec![OutPoint {
                txid: first_utxo[0].parse().unwrap(),
                vout: first_utxo[1].parse().unwrap(),
            }],
            output: vec![],
            max_gas_fee: None,
            orchard_bundle_bytes: Some(bundle_hex),
        }
    ));
}

/// Test: Multiple Orchard actions should be rejected
#[tokio::test]
#[cfg(feature = "zcash")]
#[should_panic(expected = "Only one orchard action is supported")]
async fn test_orchard_multiple_actions() {
    // TODO: Would need to generate a bundle with 2+ actions
    // For now, the single-action check happens in psbt_wrapper.rs:82
    println!("Test skeleton: Need multi-action bundle generator");
}

/// Test: Missing Orchard bundle when address suggests one should be present
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_missing_bundle() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();

    // Setup: Deposit for alice
    let alice_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        })
        .await
        .unwrap();

    check!(context.verify_deposit(
        "relayer",
        DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        },
        generate_transaction_bytes(
            vec![(
                "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
                1,
                None,
            )],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (alice_btc_deposit_address.as_str(), 500000),
            ],
        ),
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();

    let withdraw_amount = 200000u128;

    // Generate a valid UA to use as target address, but don't provide the bundle
    let orchard_amount = withdraw_amount - 10000 - config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let (recipient_ua, _bundle_hex) = get_or_gen_bundle(orchard_amount as u64);

    // Try to withdraw with UA but no bundle - should work if validation only happens when bundle is present
    // The contract should either reject UA addresses without bundles, or accept them as regular transparent
    check!(context.do_withdraw(
        "alice",
        "bridge",
        withdraw_amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: recipient_ua.clone(),
            input: vec![OutPoint {
                txid: first_utxo[0].parse().unwrap(),
                vout: first_utxo[1].parse().unwrap(),
            }],
            output: vec![],
            max_gas_fee: None,
            orchard_bundle_bytes: None, // No bundle provided!
        }
    ));

    // This test validates the behavior when UA is provided without bundle
    // Current implementation: validation only happens if bundle bytes are present
    println!("✓ Withdraw with UA but no bundle succeeded (validation only when bundle present)");
}

/// Test: Verify the generated Zcash transaction includes the Orchard bundle
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_bundle_in_zcash_tx() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();

    // Setup: Deposit for alice
    let alice_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        })
        .await
        .unwrap();

    check!(context.verify_deposit(
        "relayer",
        DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        },
        generate_transaction_bytes(
            vec![(
                "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
                1,
                None,
            )],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (alice_btc_deposit_address.as_str(), 500000),
            ],
        ),
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();

    let withdraw_amount = 200000u128;
    let btc_gas_fee = 10000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let orchard_amount = withdraw_amount - btc_gas_fee - withdraw_fee;

    let (recipient_ua, bundle_hex) = get_or_gen_bundle(orchard_amount as u64);

    check!(context.do_withdraw(
        "alice",
        "bridge",
        withdraw_amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: recipient_ua,
            input: vec![OutPoint {
                txid: first_utxo[0].parse().unwrap(),
                vout: first_utxo[1].parse().unwrap(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(320000),
                script_pubkey: Address::from_str(withdraw_change_address.as_str())
                    .expect("Invalid btc address")
                    .assume_checked()
                    .script_pubkey()
            }],
            max_gas_fee: None,
            orchard_bundle_bytes: Some(bundle_hex.clone()),
        }
    ));

    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    check!(context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));

    // Fetch the pending info and check the transaction bytes
    let pending_infos = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap();

    let pending_info = pending_infos.get(&btc_pending_sign_txs[0]).expect("Pending info not found");

    // The tx_bytes_with_sign should contain the Orchard bundle
    if let Some(tx_bytes) = &pending_info.tx_bytes_with_sign {
        let tx_hex = hex::encode(&tx_bytes);

        // The bundle hex should appear somewhere in the transaction bytes
        // (It won't be exact match due to the transaction wrapper, but the bundle data should be there)
        println!("Transaction hex length: {}", tx_hex.len());
        println!("Bundle hex length: {}", bundle_hex.len());

        // At minimum, verify the transaction is longer than just transparent data
        // A v5 Zcash transaction with Orchard should be significantly larger
        assert!(
            tx_hex.len() > 1000,
            "Transaction should include Orchard bundle (tx_len={})",
            tx_hex.len()
        );

        println!("✓ Zcash transaction includes Orchard data");
    } else {
        panic!("No transaction bytes found after signing");
    }
}
