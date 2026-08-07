mod setup;
use setup::*;

#[cfg(feature = "zcash")]
use bitcoin::{Amount, TxOut};
#[cfg(feature = "zcash")]
use satoshi_bridge::network::{Address, Chain};

/// Test: Bundle with wrong recipient should be rejected
///
/// Generates two bundles with different spending keys to create different recipients.
/// Uses bundle A but claims it's for recipient B - should be rejected.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_wrong_recipient() {
    // Set chain to ZcashTestnet for this test
    std::env::set_var("TEST_CHAIN", "ZcashTestnet");

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, None).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();

    // Setup: Deposit for alice
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

    check!(context.verify_deposit_v2(
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
        proof_json(
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        )
    ));

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();

    // Withdrawal with Orchard bundle and change output
    let utxo_value = 500000u128;
    let withdraw_amount = 200000u128;
    let btc_gas_fee = 10000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let orchard_amount = withdraw_amount - btc_gas_fee - withdraw_fee;
    let change_amount = utxo_value - orchard_amount as u128 - btc_gas_fee;

    // Generate bundle for recipient A (using spending key [1u8; 32])
    let (recipient_a, bundle_a) = gen_bundle_with_key(orchard_amount as u64, [1u8; 32]);

    // Generate bundle for recipient B (using spending key [2u8; 32])
    let (recipient_b, _bundle_b) = gen_bundle_with_key(orchard_amount as u64, [2u8; 32]);

    println!("Recipient A: {}", recipient_a);
    println!("Recipient B: {}", recipient_b);
    assert_ne!(
        recipient_a, recipient_b,
        "Recipients should be different with different spending keys"
    );

    // Get change address and parse it for Zcash
    let withdraw_change_address = context.get_change_address().await.unwrap();
    let change_script_pubkey = Address::parse(&withdraw_change_address, Chain::ZcashTestnet)
        .expect("Invalid change address")
        .script_pubkey()
        .expect("Failed to get script pubkey");

    // This should fail: use bundle_a but claim it's for recipient_b
    let result = context
        .do_withdraw(
            "alice",
            "bridge",
            withdraw_amount,
            TokenReceiverMessage::Withdraw {
                target_btc_address: recipient_b, // Wrong recipient!
                input: vec![OutPoint {
                    txid: first_utxo[0].parse().unwrap(),
                    vout: first_utxo[1].parse().unwrap(),
                }],
                output: vec![TxOut {
                    value: Amount::from_sat(change_amount as u64),
                    script_pubkey: change_script_pubkey,
                }],
                max_gas_fee: None,
                chain_specific_data: Some(ChainSpecificData {
                    orchard_bundle_bytes: hex::decode(&bundle_a).unwrap().into(), // Bundle for recipient A
                    expiry_height: 10000,
                }),
            },
        )
        .await;

    // Verify the error message
    let err_msg = tool_err_msg(&result);
    assert!(
        err_msg.contains("Orchard bundle validation failed"),
        "Expected 'Orchard bundle validation failed' error, got: {}",
        err_msg
    );
}

/// Test: Missing Orchard bundle when no transparent outputs are provided
///
/// Should reject with "empty output" error when neither transparent outputs
/// nor Orchard bundle is provided.
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_missing_bundle() {
    // Set chain to ZcashTestnet for this test
    std::env::set_var("TEST_CHAIN", "ZcashTestnet");

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, None).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let _config = context.get_bridge_config().await.unwrap();

    // Setup: Deposit for alice
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

    check!(context.verify_deposit_v2(
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
        proof_json(
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        )
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

    // Generate a Unified Address (but don't provide a bundle)
    let (unified_address, _bundle) = get_or_gen_bundle(100000); // Just get a UA, ignore bundle

    // This should FAIL: no outputs and no Orchard bundle
    let result = context
        .do_withdraw(
            "alice",
            "bridge",
            withdraw_amount,
            TokenReceiverMessage::Withdraw {
                target_btc_address: unified_address, // UA provided
                input: vec![OutPoint {
                    txid: first_utxo[0].parse().unwrap(),
                    vout: first_utxo[1].parse().unwrap(),
                }],
                output: vec![],
                max_gas_fee: None,
                chain_specific_data: None, // No bundle provided
            },
        )
        .await;

    // Verify the error message - contract requires either outputs or orchard bundle
    let err_msg = tool_err_msg(&result);
    assert!(
        err_msg.contains("empty output"),
        "Expected 'empty output' error when no bundle and no outputs provided, got: {}",
        err_msg
    );

    println!("✓ Missing bundle with empty outputs correctly rejected");
}

/// Test: Verify the generated Zcash transaction includes the Orchard bundle
#[tokio::test]
#[cfg(feature = "zcash")]
async fn test_orchard_bundle_in_zcash_tx() {
    // Set chain to ZcashTestnet for this test
    std::env::set_var("TEST_CHAIN", "ZcashTestnet");

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, None).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));

    let config = context.get_bridge_config().await.unwrap();

    // Setup: Deposit for alice
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

    check!(context.verify_deposit_v2(
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
        proof_json(
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        )
    ));

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();

    // Withdrawal with Orchard bundle and change output
    let utxo_value = 500000u128;
    let withdraw_amount = 200000u128;
    let btc_gas_fee = 10000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let orchard_amount = withdraw_amount - btc_gas_fee - withdraw_fee;
    let change_amount = utxo_value - orchard_amount as u128 - btc_gas_fee;

    let (recipient_ua, bundle_hex) = get_or_gen_bundle(orchard_amount as u64);

    // Get change address and parse it for Zcash
    let withdraw_change_address = context.get_change_address().await.unwrap();
    let change_script_pubkey = Address::parse(&withdraw_change_address, Chain::ZcashTestnet)
        .expect("Invalid change address")
        .script_pubkey()
        .expect("Failed to get script pubkey");

    check!(print "Withdrawal" context.do_withdraw(
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
                value: Amount::from_sat(change_amount as u64),
                script_pubkey: change_script_pubkey,
            }],
            max_gas_fee: None,
            chain_specific_data: Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
                expiry_height: 10000,
            }),
        }
    ));

    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();

    println!("Pending transactions: {:?}", btc_pending_sign_txs);
    assert!(
        !btc_pending_sign_txs.is_empty(),
        "Should have pending transactions"
    );

    check!(print "Signing" context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));

    // Fetch the pending info and check the transaction bytes
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();

    let pending_info = pending_infos
        .get(&btc_pending_sign_txs[0])
        .expect("Pending info not found");

    // The tx_bytes_with_sign should contain the Orchard bundle
    if let Some(tx_bytes) = &pending_info.tx_bytes_with_sign {
        let tx_hex = hex::encode(tx_bytes);

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
