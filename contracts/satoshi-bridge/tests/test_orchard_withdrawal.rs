mod setup;
use bitcoin::{Address, Amount, OutPoint, TxOut};
use satoshi_bridge::{DepositMsg, TokenReceiverMessage};
use setup::*;
use std::str::FromStr;

#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_withdrawal_with_ovk_validation() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;

    // Setup bridge fees
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();

    // Step 1: Deposit BTC to mint nBTC for alice
    let alice_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        })
        .await
        .unwrap();

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);

    check!(printr "alice deposits 500000" context.verify_deposit(
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

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 500000 - 10000);

    // Step 2: Alice initiates a withdrawal to a Zcash shielded address with Orchard bundle
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

    // Generate or get cached Orchard bundle
    let (recipient_ua, bundle_hex) = get_or_gen_bundle(orchard_amount as u64);

    println!("Testing Orchard withdrawal with UA: {}", recipient_ua);

    check!(print "alice withdraws to orchard" context.do_withdraw(
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
                value: Amount::from_sat(320000), // change
                script_pubkey: Address::from_str(withdraw_change_address.as_str())
                    .expect("Invalid btc address")
                    .assume_checked()
                    .script_pubkey()
            }],
            max_gas_fee: None,
            orchard_bundle_bytes: Some(bundle_hex),
        }
    ));

    // The PsbtWrapper should:
    // 1. Parse the orchard bundle ✓
    // 2. Verify it has exactly 1 action ✓
    // 3. Recover the output using BRIDGE_OVK ✓
    // 4. Verify the recovered amount matches orchard_amount ✓
    // 5. Verify the recovered recipient matches the UA's Orchard receiver ✓

    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    // Sign and verify the transaction
    check!(print "sign transaction" context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));

    let btc_pending_verify_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    check!(print "verify withdraw" context.verify_withdraw(
        "relayer",
        &btc_pending_verify_txs[0],
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));

    // Verify nBTC was burned
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        500000 - 10000 - withdraw_amount
    );
}

#[tokio::test]
#[cfg(feature = "zcash")]
#[should_panic(expected = "Orchard amount mismatch")]
async fn test_orchard_withdrawal_amount_mismatch() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();

    // Deposit for alice
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

    // Generate bundle with WRONG amount (different from what we're withdrawing)
    let wrong_amount = 100000u64; // Different from orchard_amount
    let (_recipient_ua, bundle_hex) = get_or_gen_bundle(wrong_amount);

    // This should panic with "Orchard amount mismatch"
    check!(context.do_withdraw(
        "alice",
        "bridge",
        withdraw_amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: "u1test...".to_string(), // Will use bundle's actual recipient
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
