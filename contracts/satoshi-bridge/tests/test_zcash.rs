#![cfg(feature = "zcash")]
use near_workspaces::network::Sandbox;
use near_workspaces::Worker;
use satoshi_bridge::TokenReceiverMessage;
use std::str::FromStr;

mod setup;
use setup::context::Context;

#[tokio::test]
async fn test_withdraw_zcash_shielded_policy() {
    // Generate UA + Orchard bundle (single action, OVK=00..00) for 50_000 zats.
    let (ua, bundle_hex) = setup::orchard::gen_ua_and_orchard_bundle_hex(50_000, "test");

    let worker: Worker<Sandbox> = near_workspaces::sandbox().await.unwrap();
    let context = Context::new_with_chain(&worker, "ZcashTestnet").await;

    // Ensure storage paid where needed.
    let _ = context.storage_deposit("nbtc", "alice").await.unwrap();

    // Seed a deposit UTXO and mint nBTC to Alice so she can withdraw.
    let alice_deposit_addr = context
        .get_user_deposit_address(satoshi_bridge::DepositMsg {
            recipient_id: context.get_account_by_name("alice").id().clone(),
            post_actions: None,
            extra_msg: None,
        })
        .await
        .unwrap();
    // Create a tx that pays 250_000 to Alice’s deposit address at vout=1.
    // For Zcash chain, convert UA/transparent address strings to script_pubkeys using the bridge.
    let cfg = context.get_bridge_config().await.unwrap();
    let spk_deposit = cfg.string_to_script_pubkey(&alice_deposit_addr);
    let spk_other = cfg.string_to_script_pubkey("tmJpMbYtRf9Hgi8HUJ4FGkoM3FUSHsu28wM");
    let tx = bitcoin::Transaction {
        version: bitcoin::transaction::Version::TWO,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![{
            let mut tx_in = bitcoin::TxIn::default();
            tx_in.previous_output.txid = "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234"
                .parse()
                .unwrap();
            tx_in.previous_output.vout = 1;
            tx_in.sequence.0 = 4294967293;
            tx_in
        }],
        output: vec![
            bitcoin::TxOut { value: bitcoin::Amount::from_sat(90000), script_pubkey: spk_other },
            bitcoin::TxOut { value: bitcoin::Amount::from_sat(250000), script_pubkey: spk_deposit },
        ],
    };
    let tx_bytes = bitcoin::consensus::serialize(&tx);
    // Instead of full verify_deposit, seed the UTXO directly (test-only path).
    let deposit_msg = satoshi_bridge::DepositMsg {
        recipient_id: context.get_account_by_name("alice").id().clone(),
        post_actions: None,
        extra_msg: None,
    };
    context
        .get_account_by_name("root")
        .call(context.get_account_by_name("bridge").id(), "test_seed_utxo")
        .args_json(near_sdk::serde_json::json!({
            "deposit_msg": deposit_msg,
            "txid": "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
            "vout": 1u32,
            "amount": "250000",
        }))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // Build change-only outputs. Select the first available UTXO and spend it.
    let utxos = context.get_utxos_paged().await.unwrap();
    let (key, utxo) = utxos.iter().next().expect("at least one UTXO");
    let parts: Vec<&str> = key.split('@').collect();
    let input = vec![bitcoin::OutPoint {
        txid: parts[0].parse().unwrap(),
        vout: parts[1].parse().unwrap(),
    }];
    let input_amount = utxo.balance;

    // Withdraw parameters: orchard_amount=50_000, miner_fee=10_000, withdraw_fee=50_000.
    // amount = orchard_amount + miner_fee + withdraw_fee = 110_000.
    let withdraw_amount: u128 = 110_000;
    let miner_fee = 10_000u64;
    let change_sum = input_amount - miner_fee;

    // Create change outputs summing to change_sum.
    let change_addr = context.get_change_address().await.unwrap();
    let change_spk = cfg.string_to_script_pubkey(&change_addr);
    let mut remaining = change_sum;
    let mut outputs: Vec<bitcoin::TxOut> = Vec::new();
    // Split into up to 3 outputs for variety.
    for _ in 0..2 {
        let piece = remaining / 2;
        if piece == 0 {
            break;
        }
        outputs.push(bitcoin::TxOut { value: bitcoin::Amount::from_sat(piece), script_pubkey: change_spk.clone() });
        remaining -= piece;
    }
    if remaining > 0 {
        outputs.push(bitcoin::TxOut { value: bitcoin::Amount::from_sat(remaining), script_pubkey: change_spk });
    }

    // Execute withdraw with Orchard bundle attached (external verifier does proof+policy).
    let outcome = context
        .do_withdraw(
            "alice",
            "bridge",
            withdraw_amount,
            TokenReceiverMessage::Withdraw {
                target_btc_address: ua,
                input,
                output: outputs,
                max_gas_fee: None,
                orchard_bundle_bytes: Some(bundle_hex),
            },
        )
        .await
        .unwrap();

    // Report gas usage for this transaction (total over all receipts).
    println!(
        "Orchard withdraw total_gas_burnt: {}",
        outcome.total_gas_burnt
    );

    // Assert state moved to PendingSign and pending info created.
    assert!(context
        .get_account("alice")
        .await
        .unwrap()
        .unwrap()
        .btc_pending_sign_id
        .is_some());
    let pending = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending.len(), 1);
    let values = pending.values().cloned().collect::<Vec<_>>();
    values[0].assert_pending_sign();
}
