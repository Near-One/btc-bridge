mod setup;
use bitcoin::OutPoint;
use satoshi_bridge::{DepositMsg, TokenReceiverMessage};
use setup::*;

#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_withdrawal_with_ovk_validation() {
    // Set chain to ZcashTestnet for this test
    std::env::set_var("TEST_CHAIN", "ZcashTestnet");

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;

    // Setup bridge fees
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();

    // Verify we're on Zcash chain
    println!("Testing on chain: {:?}", config.chain);
    assert_eq!(format!("{:?}", config.chain), "ZcashTestnet");

    // Deposit for alice using Zcash transaction format
    let alice_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        })
        .await
        .unwrap();

    println!("Alice deposit address: {}", alice_btc_deposit_address);

    // Generate Zcash v5 transaction for deposit
    let zcash_tx_bytes = setup::utils::generate_zcash_transaction_bytes(
        vec![(
            "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
            1,
            None,
        )],
        vec![
            ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
            (alice_btc_deposit_address.as_str(), 500000),
        ],
    );

    println!(
        "Generated Zcash transaction, size: {} bytes",
        zcash_tx_bytes.len()
    );

    // Verify the deposit using Zcash transaction
    check!(context.verify_deposit(
        "relayer",
        DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        },
        zcash_tx_bytes,
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));

    println!("✅ Deposit verified successfully");

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();

    assert!(!utxos_keys.is_empty(), "Should have UTXOs after deposit");
    println!("UTXOs: {:?}", utxos_keys);

    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();

    // Now test withdrawal with Orchard bundle
    // Use a larger amount to ensure orchard_amount > 0 after fees
    let withdraw_amount = 400000u128; // 400000 satoshis (close to the 500000 we deposited)
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let btc_gas_fee = 10000u128;
    let orchard_amount = (withdraw_amount - withdraw_fee - btc_gas_fee) as u64;

    println!(
        "Withdraw fee: {}, BTC gas fee: {}",
        withdraw_fee, btc_gas_fee
    );

    println!(
        "Withdraw amount: {}, Orchard amount: {}",
        withdraw_amount, orchard_amount
    );

    // Generate Orchard bundle with correct amount
    let (recipient_ua, bundle_hex) = setup::orchard::get_or_gen_bundle(orchard_amount);
    println!("Generated Orchard bundle for recipient: {}", recipient_ua);
    println!("Bundle size: {} bytes", bundle_hex.len() / 2);

    // Perform withdrawal with Orchard bundle
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
            orchard_bundle_bytes: Some(bundle_hex),
        }
    ));

    println!("✅ Withdrawal with Orchard bundle completed successfully");
}

#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_withdrawal_amount_mismatch() {
    // Set chain to ZcashTestnet for this test
    std::env::set_var("TEST_CHAIN", "ZcashTestnet");

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();

    // Deposit for alice using Zcash transaction format
    let alice_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        })
        .await
        .unwrap();

    // Generate Zcash v5 transaction for deposit
    let zcash_tx_bytes = setup::utils::generate_zcash_transaction_bytes(
        vec![(
            "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
            1,
            None,
        )],
        vec![
            ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
            (alice_btc_deposit_address.as_str(), 500000),
        ],
    );

    check!(context.verify_deposit(
        "relayer",
        DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        },
        zcash_tx_bytes,
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

    // Use a larger amount to ensure orchard_amount > 0 after fees
    let withdraw_amount = 400000u128;
    let btc_gas_fee = 10000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);

    // Calculate expected orchard amount
    let expected_orchard_amount = (withdraw_amount - withdraw_fee - btc_gas_fee) as u64;

    // Generate bundle with WRONG amount (different from what we're withdrawing)
    let wrong_amount = 100000u64; // Different from expected_orchard_amount
    let (_recipient_ua, bundle_hex) = get_or_gen_bundle(wrong_amount);

    println!(
        "Expected orchard amount: {}, Using wrong amount: {}",
        expected_orchard_amount, wrong_amount
    );

    // This should fail with "Orchard amount mismatch"
    let result = context
        .do_withdraw(
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
            },
        )
        .await;

    // Check that it failed with the expected error
    assert!(
        result.is_ok(),
        "Withdrawal call should not fail at network level"
    );
    let outcome = result.unwrap();
    assert!(
        !outcome.is_success() || !outcome.receipt_failures().is_empty(),
        "Withdrawal should fail due to Orchard validation"
    );

    // Check the error message contains "Orchard amount mismatch"
    let err_msg = setup::utils::tool_err_msg(&Ok(outcome));
    assert!(
        err_msg.contains("Orchard amount mismatch"),
        "Expected 'Orchard amount mismatch' error, got: {}",
        err_msg
    );
}
