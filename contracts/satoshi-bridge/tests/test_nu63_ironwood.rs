#![cfg(feature = "zcash")]

mod setup;
use setup::*;

use bitcoin::{Amount, TxOut};
use near_sdk::serde_json::json;
use satoshi_bridge::network::{Address, Chain};
use std::collections::HashMap;

/// ZcashTestnet activates NU6.3 ("Ironwood", branch id 0x37a5165b) at 4_134_000.
const PRE_NU6_3_HEIGHT: u32 = 4_120_000;
const POST_NU6_3_HEIGHT: u32 = 4_200_000;

const DEPOSIT_TXID: &str = "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234";
const BLOCK_HASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";
const ZEC_TADDR: &str = "tmD67UTsZ4iBbhCae4D43k1x8fhFNhwd4Jn";

async fn deposit_for_alice(context: &Context, amount: u64) -> (String, String) {
    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
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

    check!(context.verify_deposit(
        "relayer",
        deposit_msg,
        zcash_tx_bytes,
        1,
        BLOCK_HASH.to_string(),
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
    assert!(!utxos_keys.is_empty(), "Should have UTXOs after deposit");
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    (first_utxo[0].to_string(), first_utxo[1].to_string())
}

/// Shielded withdrawal after NU6.3 activation: the bundle is an Ironwood bundle
/// (V3 notes, post-NU6.3 circuit), the bridge builds a v6 transaction with the
/// bundle in the Ironwood slot, and signing runs the v6 (ZIP 244 + Ironwood
/// digest) sighash path end-to-end.
#[tokio::test]
async fn test_ironwood_withdrawal_v6_sign() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    let config = context.get_bridge_config().await.unwrap();

    let (utxo_txid, utxo_vout) = deposit_for_alice(&context, 500_000).await;

    context.set_light_client_block_height(POST_NU6_3_HEIGHT).await;

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
                expiry_height: POST_NU6_3_HEIGHT + 10_000,
            }),
        }
    ));

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending_infos.len(), 1);
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    pending_infos.values().next().unwrap().assert_pending_sign();

    check!(context.sign_btc_transaction("alice", &pending_keys[0], 0, 0));

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    pending_infos.values().next().unwrap().assert_pending_verify();
}

/// Shielded refund executed after NU6.3 activation: the refund transaction is
/// built as v6 with an Ironwood bundle, signed, and finalized.
#[tokio::test]
async fn test_ironwood_shielded_refund() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

    let deposit_amount: u64 = 180_000;
    let gas_fee: u64 = 10_000; // default = ZIP-317 minimum (same for the Ironwood slot)
    let refund_amount: u64 = deposit_amount - gas_fee;

    // Same UA/receiver as the legacy fixtures — NU6.3 changes no address
    // encodings; only the bundle (and the pool the funds land in) differs.
    let (recipient_ua, bundle_hex) = setup::orchard::get_or_gen_ironwood_bundle(refund_amount);

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(recipient_ua.clone()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let tx_bytes = setup::utils::generate_transaction_bytes(
        vec![(DEPOSIT_TXID, 1, None)],
        vec![(deposit_address.as_str(), deposit_amount)],
    );

    check!(
        print "request_refund"
        context.request_refund(
            "relayer",
            deposit_msg.clone(),
            &recipient_ua,
            tx_bytes.clone(),
            0,
            BLOCK_HASH.to_string(),
            1,
            vec![],
            None,
        )
    );

    let requests: HashMap<String, near_sdk::serde_json::Value> = context
        .bridge_contract
        .call("get_refund_requests_paged")
        .args_json(json!({}))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(requests.len(), 1, "exactly one refund request expected");
    let key = requests.keys().next().unwrap().clone();

    context.set_light_client_block_height(POST_NU6_3_HEIGHT).await;

    check!(
        print "execute_refund"
        context.execute_refund(
            "root",
            &key,
            Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
                expiry_height: 0,
            }),
        )
    );

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending_infos.len(), 1);
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    pending_infos.values().next().unwrap().assert_pending_sign();

    check!(
        print "sign_btc_transaction"
        context.sign_btc_transaction("alice", &pending_keys[0], 0, 0)
    );

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    pending_infos.values().next().unwrap().assert_pending_verify();

    check!(
        print "verify_refund_finalize"
        context.verify_refund_finalize(
            "relayer",
            &pending_keys[0],
            BLOCK_HASH.to_string(),
            1,
            vec![],
        )
    );

    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

/// A transparent withdrawal built in the Nu6_2 epoch (v5, branch id Nu6_2) is
/// unmineable once NU6.3 activates (a v5 tx commits to its consensus branch id).
/// The recovery path is user RBF: the replacement must pick up the branch id
/// from the CURRENT light-client height (Nu6_3 -> v6), yield a different txid,
/// and pass the v6 sighash path when signed.
#[tokio::test]
async fn test_transparent_withdraw_rbf_across_nu63_activation() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some("ZcashTestnet".to_string())).await;

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

    // Withdrawal created before activation: v5 tx with branch id Nu6_2.
    context.set_light_client_block_height(PRE_NU6_3_HEIGHT).await;

    let utxo_value = 500_000u128;
    let withdraw_amount = 200_000u128;
    let btc_gas_fee = 10_000u128;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let target_amount = withdraw_amount - withdraw_fee - btc_gas_fee;
    let change_amount = utxo_value - target_amount - btc_gas_fee;

    let target_script_pubkey = Address::parse(ZEC_TADDR, Chain::ZcashTestnet)
        .expect("Invalid target address")
        .script_pubkey()
        .expect("Failed to get script pubkey");
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
            target_btc_address: ZEC_TADDR.to_string(),
            input: vec![OutPoint {
                txid: utxo_txid.parse().unwrap(),
                vout: utxo_vout.parse().unwrap(),
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(target_amount as u64),
                    script_pubkey: target_script_pubkey,
                },
                TxOut {
                    value: Amount::from_sat(change_amount as u64),
                    script_pubkey: change_script_pubkey,
                },
            ],
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
    pending_infos.values().next().unwrap().assert_pending_verify();

    // NU6.3 activates; the signed v5/Nu6_2 tx above can never be mined now.
    context.set_light_client_block_height(POST_NU6_3_HEIGHT).await;

    // User RBF: empty `output` reuses the original outputs; the branch id (and
    // hence tx version and txid) comes from the current height.
    check!(
        print "withdraw_rbf"
        context
            .get_account_by_name("alice")
            .call(context.bridge_contract.id(), "withdraw_rbf")
            .args_json(json!({
                "original_btc_pending_verify_id": original_id,
                "output": [],
                "chain_specific_data": null,
            }))
            .max_gas()
            .transact()
    );

    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(
        pending_infos.len(),
        2,
        "RBF must create a second, distinct pending tx"
    );
    let rbf_id = pending_infos
        .keys()
        .find(|k| **k != original_id)
        .expect("replacement tx id")
        .clone();
    assert_ne!(
        rbf_id, original_id,
        "the replacement must have a different txid (different branch id)"
    );
    pending_infos[&rbf_id].assert_pending_sign();

    // Signing the replacement exercises the v6 sighash for a transparent-only tx.
    check!(context.sign_btc_transaction("alice", &rbf_id, 0, 0));
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    pending_infos[&rbf_id].assert_pending_verify();
}
