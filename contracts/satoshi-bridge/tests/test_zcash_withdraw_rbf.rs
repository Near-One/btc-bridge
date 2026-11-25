#![cfg(feature = "zcash")]

mod setup;
use setup::*;

use bitcoin::{Amount, ScriptBuf, TxOut};
use near_sdk::serde_json::json;
use near_workspaces::result::ExecutionFinalResult;
use satoshi_bridge::network::{Address, Chain};

const DEPOSIT_TXID: &str = "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234";
const BLOCK_HASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";
const ZEC_TADDR: &str = "tmD67UTsZ4iBbhCae4D43k1x8fhFNhwd4Jn";

const UTXO_VALUE: u128 = 500_000;
const WITHDRAW_AMOUNT: u128 = 200_000;
const BTC_GAS_FEE: u128 = 10_000;
const ORIGINAL_EXPIRY_HEIGHT: u32 = 10_000;

fn script_pubkey(address: &str) -> ScriptBuf {
    Address::parse(address, Chain::ZcashTestnet)
        .expect("Invalid address")
        .script_pubkey()
        .expect("Failed to get script pubkey")
}

async fn setup_context(
    worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
) -> Context {
    let context = Context::new(worker, Some("ZcashTestnet".to_string())).await;
    check!(context.storage_deposit("nbtc", "alice"));
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    context
}

async fn deposit_for_alice(context: &Context) -> OutPoint {
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

    check!(context.verify_deposit_v2(
        "relayer",
        deposit_msg,
        setup::utils::generate_transaction_bytes(
            vec![(DEPOSIT_TXID, 1, None)],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (deposit_address.as_str(), UTXO_VALUE as u64),
            ],
        ),
        1,
        proof_json(BLOCK_HASH.to_string(), 1, vec![])
    ));

    let utxo_key = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .next()
        .expect("Should have UTXO after deposit")
        .clone();
    let (txid, vout) = utxo_key.split_once('@').unwrap();
    OutPoint {
        txid: txid.parse().unwrap(),
        vout: vout.parse().unwrap(),
    }
}

async fn withdraw_and_sign(
    context: &Context,
    target_address: &str,
    output: Vec<TxOut>,
    chain_specific_data: Option<ChainSpecificData>,
) -> String {
    let input = deposit_for_alice(context).await;
    check!(context.do_withdraw(
        "alice",
        "bridge",
        WITHDRAW_AMOUNT,
        TokenReceiverMessage::Withdraw {
            target_btc_address: target_address.to_string(),
            input: vec![input],
            output,
            max_gas_fee: None,
            chain_specific_data,
        }
    ));
    let original_id = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    check!(context.sign_btc_transaction("alice", &original_id, 0, 0));
    original_id
}

fn transparent_outputs(
    change_script_pubkey: &ScriptBuf,
    withdraw_fee: u128,
    gas_fee: u128,
) -> Vec<TxOut> {
    vec![
        TxOut {
            value: Amount::from_sat((WITHDRAW_AMOUNT - withdraw_fee - gas_fee) as u64),
            script_pubkey: script_pubkey(ZEC_TADDR),
        },
        TxOut {
            value: Amount::from_sat((UTXO_VALUE - WITHDRAW_AMOUNT + withdraw_fee) as u64),
            script_pubkey: change_script_pubkey.clone(),
        },
    ]
}

async fn withdraw_rbf(
    context: &Context,
    original_id: &str,
    output: Vec<TxOut>,
    chain_specific_data: Option<ChainSpecificData>,
) -> near_workspaces::Result<ExecutionFinalResult> {
    context
        .get_account_by_name("alice")
        .call(context.bridge_contract.id(), "withdraw_rbf")
        .args_json(json!({
            "original_btc_pending_verify_id": original_id,
            "output": output,
            "chain_specific_data": chain_specific_data,
        }))
        .deposit(near_sdk::NearToken::from_near(1))
        .max_gas()
        .transact()
        .await
}

async fn pending_infos_count(context: &Context) -> usize {
    context.get_btc_pending_infos_paged().await.unwrap().len()
}

#[tokio::test]
async fn test_zcash_withdraw_rbf_rejects_unchanged_expiry_height() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let withdraw_fee = context
        .get_bridge_config()
        .await
        .unwrap()
        .withdraw_bridge_fee
        .get_fee(WITHDRAW_AMOUNT);
    let change_script_pubkey = script_pubkey(&context.get_change_address().await.unwrap());
    let output = transparent_outputs(&change_script_pubkey, withdraw_fee, BTC_GAS_FEE);
    let original_id = withdraw_and_sign(&context, ZEC_TADDR, output.clone(), None).await;

    check!(
        withdraw_rbf(&context, &original_id, vec![], None),
        "Expiry height must increase"
    );
    check!(
        withdraw_rbf(&context, &original_id, output, None),
        "Expiry height must increase"
    );
    assert_eq!(pending_infos_count(&context).await, 1);
}

#[tokio::test]
async fn test_zcash_withdraw_rbf_rejects_changed_outputs() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let withdraw_fee = context
        .get_bridge_config()
        .await
        .unwrap()
        .withdraw_bridge_fee
        .get_fee(WITHDRAW_AMOUNT);
    let change_script_pubkey = script_pubkey(&context.get_change_address().await.unwrap());
    let output = transparent_outputs(&change_script_pubkey, withdraw_fee, BTC_GAS_FEE);
    let original_id = withdraw_and_sign(&context, ZEC_TADDR, output.clone(), None).await;

    context.set_light_client_block_height(1001).await;

    check!(
        withdraw_rbf(
            &context,
            &original_id,
            transparent_outputs(&change_script_pubkey, withdraw_fee, BTC_GAS_FEE + 1000),
            None
        ),
        "Outputs must not change"
    );
    check!(
        withdraw_rbf(
            &context,
            &original_id,
            transparent_outputs(&change_script_pubkey, withdraw_fee, BTC_GAS_FEE - 1000),
            None
        ),
        "Outputs must not change"
    );
    assert_eq!(pending_infos_count(&context).await, 1);

    check!(withdraw_rbf(&context, &original_id, output, None));
    assert_eq!(pending_infos_count(&context).await, 2);
}

#[tokio::test]
async fn test_zcash_withdraw_rbf_rejects_changed_orchard_output() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let withdraw_fee = context
        .get_bridge_config()
        .await
        .unwrap()
        .withdraw_bridge_fee
        .get_fee(WITHDRAW_AMOUNT);
    let orchard_amount = (WITHDRAW_AMOUNT - withdraw_fee - BTC_GAS_FEE) as u64;
    let change_amount = UTXO_VALUE - orchard_amount as u128 - BTC_GAS_FEE;
    let (recipient_ua, bundle_hex) = get_or_gen_bundle(orchard_amount);
    let (_, other_amount_bundle_hex) = get_or_gen_bundle(100_000);
    let (_, other_recipient_bundle_hex) = gen_bundle_with_key(orchard_amount, [1u8; 32]);
    let change_script_pubkey = script_pubkey(&context.get_change_address().await.unwrap());

    let orchard_data = |bundle_hex: &str, expiry_height: u32| {
        Some(ChainSpecificData {
            orchard_bundle_bytes: hex::decode(bundle_hex).unwrap().into(),
            expiry_height,
        })
    };

    let original_id = withdraw_and_sign(
        &context,
        &recipient_ua,
        vec![TxOut {
            value: Amount::from_sat(change_amount as u64),
            script_pubkey: change_script_pubkey,
        }],
        orchard_data(&bundle_hex, ORIGINAL_EXPIRY_HEIGHT),
    )
    .await;

    check!(
        withdraw_rbf(
            &context,
            &original_id,
            vec![],
            orchard_data(&other_amount_bundle_hex, ORIGINAL_EXPIRY_HEIGHT + 1)
        ),
        "Orchard output must not change"
    );
    check!(
        withdraw_rbf(
            &context,
            &original_id,
            vec![],
            orchard_data(&other_recipient_bundle_hex, ORIGINAL_EXPIRY_HEIGHT + 1)
        ),
        "Orchard output must not change"
    );
    check!(
        withdraw_rbf(
            &context,
            &original_id,
            vec![],
            orchard_data(&bundle_hex, ORIGINAL_EXPIRY_HEIGHT)
        ),
        "Expiry height must increase"
    );
    assert_eq!(pending_infos_count(&context).await, 1);

    check!(withdraw_rbf(
        &context,
        &original_id,
        vec![],
        orchard_data(&bundle_hex, ORIGINAL_EXPIRY_HEIGHT + 1)
    ));
    assert_eq!(pending_infos_count(&context).await, 2);
}

#[tokio::test]
async fn test_zcash_withdraw_rbf_refunds_storage_deposit_when_rejected() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let withdraw_fee = context
        .get_bridge_config()
        .await
        .unwrap()
        .withdraw_bridge_fee
        .get_fee(WITHDRAW_AMOUNT);
    let change_script_pubkey = script_pubkey(&context.get_change_address().await.unwrap());
    let output = transparent_outputs(&change_script_pubkey, withdraw_fee, BTC_GAS_FEE);
    let original_id = withdraw_and_sign(&context, ZEC_TADDR, output.clone(), None).await;

    worker.fast_forward(5).await.unwrap();
    let alice = context.get_account_by_name("alice");
    let alice_balance_before = alice.view_account().await.unwrap().balance;
    check!(
        withdraw_rbf(&context, &original_id, output, None),
        "Expiry height must increase"
    );
    worker.fast_forward(5).await.unwrap();

    let alice_spent =
        alice_balance_before.saturating_sub(alice.view_account().await.unwrap().balance);
    let required_balance = context.required_balance_for_withdraw_rbf().await.unwrap();
    assert!(alice_spent < required_balance);
    assert_eq!(pending_infos_count(&context).await, 1);
}

#[tokio::test]
async fn test_zcash_orchard_withdraw_rbf_succeeds_with_minimal_gas() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let withdraw_fee = context
        .get_bridge_config()
        .await
        .unwrap()
        .withdraw_bridge_fee
        .get_fee(WITHDRAW_AMOUNT);
    let orchard_amount = (WITHDRAW_AMOUNT - withdraw_fee - BTC_GAS_FEE) as u64;
    let change_amount = UTXO_VALUE - orchard_amount as u128 - BTC_GAS_FEE;
    let (recipient_ua, bundle_hex) = get_or_gen_bundle(orchard_amount);
    let change_script_pubkey = script_pubkey(&context.get_change_address().await.unwrap());
    let orchard_data = |expiry_height: u32| {
        Some(ChainSpecificData {
            orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
            expiry_height,
        })
    };
    let original_id = withdraw_and_sign(
        &context,
        &recipient_ua,
        vec![TxOut {
            value: Amount::from_sat(change_amount as u64),
            script_pubkey: change_script_pubkey,
        }],
        orchard_data(ORIGINAL_EXPIRY_HEIGHT),
    )
    .await;

    let mut tgas = 100;
    let outcome = loop {
        let outcome = context
            .get_account_by_name("alice")
            .call(context.bridge_contract.id(), "withdraw_rbf")
            .args_json(json!({
                "original_btc_pending_verify_id": original_id,
                "output": Vec::<TxOut>::new(),
                "chain_specific_data": orchard_data(ORIGINAL_EXPIRY_HEIGHT + 1),
            }))
            .deposit(near_sdk::NearToken::from_near(1))
            .gas(near_sdk::Gas::from_tgas(tgas))
            .transact()
            .await
            .unwrap();
        if outcome.is_success() {
            break outcome;
        }
        assert_eq!(pending_infos_count(&context).await, 1);
        tgas += 2;
        assert!(tgas <= 300);
    };

    assert!(outcome.receipt_failures().is_empty());
    assert_eq!(pending_infos_count(&context).await, 2);
}
