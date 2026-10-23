#![cfg(feature = "zcash")]

mod setup;
use setup::*;

use bitcoin::{Amount, TxOut};
use near_sdk::serde_json::json;
use satoshi_bridge::network::{Address, Chain};

/// ZcashTestnet activates NU7 (branch id 0x77190AD9) at 4_465_026 per ZIP-0259.
/// `PRE_NU7_HEIGHT` sits in the NU6.3 epoch (activated at 4_134_000), so the two
/// heights differ only in consensus branch id: both build v6 transactions and
/// both carry the shielded bundle in the Ironwood slot.
const PRE_NU7_HEIGHT: u32 = 4_400_000;
const POST_NU7_HEIGHT: u32 = 4_500_000;

const DEPOSIT_TXID: &str = "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234";
const BLOCK_HASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";
const ZEC_TADDR: &str = "tmD67UTsZ4iBbhCae4D43k1x8fhFNhwd4Jn";

async fn deposit_for_alice(context: &Context, amount: u64) -> (String, String) {
    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: Some(satoshi_bridge::SafeDepositMsg { msg: String::new() }),
        refund_address: None,
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let zcash_tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(DEPOSIT_TXID, 1, None)],
        vec![
            ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
            (deposit_address.as_str(), amount),
        ],
    );

    check!(context.verify_deposit_v2(
        "relayer",
        deposit_msg,
        zcash_tx_bytes,
        1,
        proof_json(BLOCK_HASH.to_string(), 1, vec![])
    ));

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    assert!(!utxos_keys.is_empty(), "Should have UTXOs after deposit");
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    (first_utxo[0].to_string(), first_utxo[1].to_string())
}

/// Shielded withdrawal after NU7 activation: NU7 keeps the NU6.3 transaction
/// shape (v6 + Ironwood slot, Orchard protocol revision V3), so the same
/// Ironwood bundle is accepted, but the tx commits to branch id 0x77190AD9.
/// Exercises build -> sign (v6 ZIP 244 + Ironwood digest) end to end.
#[tokio::test]
async fn test_nu7_shielded_withdrawal_v6_sign() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;
    check!(context.storage_deposit("nbtc", "alice"));

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    let config = context.get_bridge_config().await.unwrap();

    let (utxo_txid, utxo_vout) = deposit_for_alice(&context, 500_000).await;

    context.set_light_client_block_height(POST_NU7_HEIGHT).await;

    let utxo_value = 500_000u128;
    let withdraw_amount = 200_000u128;
    let btc_gas_fee = 10_000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let shielded_amount = (withdraw_amount - withdraw_fee - btc_gas_fee) as u64;
    let change_amount = utxo_value - shielded_amount as u128 - btc_gas_fee;

    let (recipient_ua, bundle_hex) = setup::orchard::get_or_gen_ironwood_bundle(shielded_amount);

    let withdraw_change_address = context.get_change_address().await.unwrap();
    let change_script_pubkey = Address::parse(&withdraw_change_address, Chain::ZcashTestnet)
        .expect("Invalid change address")
        .script_pubkey()
        .expect("Failed to get script pubkey");

    check!(context.do_withdraw(
        "alice",
        "bridge",
        withdraw_amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: recipient_ua.clone(),
            input: vec![OutPoint {
                txid: utxo_txid.parse().unwrap(),
                vout: utxo_vout.parse().unwrap(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(change_amount as u64),
                script_pubkey: change_script_pubkey,
            }],
            max_gas_fee: None,
            chain_specific_data: Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
                expiry_height: POST_NU7_HEIGHT + 10_000,
            }),
        }
    ));

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending_infos.len(), 1);
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    pending_infos.values().next().unwrap().assert_pending_sign();

    check!(context.sign_btc_transaction("alice", &pending_keys[0], 0, 0));

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    pending_infos
        .values()
        .next()
        .unwrap()
        .assert_pending_verify();
}

/// A transparent withdrawal built in the NU6.3 epoch commits to branch id
/// 0x37A5165B and becomes unmineable once NU7 activates. The recovery path is
/// user RBF: the replacement picks the branch id up from the CURRENT
/// light-client height (NU7), which changes the sighash and therefore the txid,
/// even though every other field is identical. A replacement whose txid matched
/// the original would mean the branch id never reached the transaction.
#[tokio::test]
async fn test_transparent_withdraw_rbf_across_nu7_activation() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;
    check!(context.storage_deposit("nbtc", "alice"));

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    let config = context.get_bridge_config().await.unwrap();

    let (utxo_txid, utxo_vout) = deposit_for_alice(&context, 500_000).await;

    // Allow alice to hold the original + the RBF replacement.
    let alice_id = context.get_account_by_name("alice").id().clone();
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "set_pending_tx_limit")
        .args_json(json!({ "account_id": alice_id, "max_pending": 2 }))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // Withdrawal created before activation: v6 tx with branch id Nu6_3.
    context.set_light_client_block_height(PRE_NU7_HEIGHT).await;

    let utxo_value = 500_000u128;
    let withdraw_amount = 200_000u128;
    let btc_gas_fee = 10_000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let recipient_amount = withdraw_amount - withdraw_fee - btc_gas_fee;
    let change_amount = utxo_value - recipient_amount - btc_gas_fee;

    let recipient_script_pubkey = Address::parse(ZEC_TADDR, Chain::ZcashTestnet)
        .expect("Invalid recipient address")
        .script_pubkey()
        .expect("Failed to get script pubkey");
    let withdraw_change_address = context.get_change_address().await.unwrap();
    let change_script_pubkey = Address::parse(&withdraw_change_address, Chain::ZcashTestnet)
        .expect("Invalid change address")
        .script_pubkey()
        .expect("Failed to get script pubkey");

    let outputs = vec![
        TxOut {
            value: Amount::from_sat(recipient_amount as u64),
            script_pubkey: recipient_script_pubkey,
        },
        TxOut {
            value: Amount::from_sat(change_amount as u64),
            script_pubkey: change_script_pubkey,
        },
    ];

    check!(context.do_withdraw(
        "alice",
        "bridge",
        withdraw_amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: ZEC_TADDR.to_string(),
            input: vec![OutPoint {
                txid: utxo_txid.parse().unwrap(),
                vout: utxo_vout.parse().unwrap(),
            }],
            output: outputs,
            max_gas_fee: None,
            chain_specific_data: None,
        }
    ));

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending_infos.len(), 1);
    let original_id = pending_infos.keys().next().unwrap().clone();
    pending_infos.values().next().unwrap().assert_pending_sign();

    check!(context.sign_btc_transaction("alice", &original_id, 0, 0));
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    pending_infos[&original_id].assert_pending_verify();

    // NU7 activates; the signed Nu6_3 tx above can never be mined now.
    context.set_light_client_block_height(POST_NU7_HEIGHT).await;

    check!(
        print "withdraw_rbf (nu7)"
        context
            .get_account_by_name("alice")
            .call(context.bridge_contract.id(), "withdraw_rbf")
            .args_json(json!({
                "original_btc_pending_verify_id": original_id,
                "output": [],
                "chain_specific_data": null,
            }))
            .deposit(near_sdk::NearToken::from_near(1))
            .max_gas()
            .transact()
    );

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(
        pending_infos.len(),
        2,
        "the NU7 replacement must be a distinct transaction, not a no-op rebuild"
    );
    let rbf_id = pending_infos
        .keys()
        .find(|k| **k != original_id)
        .expect("replacement tx id")
        .clone();
    pending_infos[&rbf_id].assert_pending_sign();

    // Signing the replacement exercises the v6 sighash under branch id Nu7.
    check!(context.sign_btc_transaction("alice", &rbf_id, 0, 0));
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    pending_infos[&rbf_id].assert_pending_verify();
}
