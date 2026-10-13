#![cfg(not(feature = "zcash"))]

mod setup;
use bitcoin::{OutPoint, TxOut};
use satoshi_bridge::network::Chain;
use satoshi_bridge::{DepositMsg, PendingInfoState, TokenReceiverMessage};
use setup::*;

const CHAIN: &str = "BitcoinMainnet";
const TARGET_ADDRESS: &str = "1PAGsaT5vDz6hjzvuenSw33hWzESTR3ZHQ";
const BLOCKHASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";

const DEPOSIT_AMOUNT: u64 = 500000;
const WITHDRAW_AMOUNT: u128 = 200000;
const NEW_WITHDRAW_AMOUNT: u128 = 100000;
const BTC_GAS_FEE: u128 = 10000;
const RBF_BTC_GAS_FEE: u128 = 20000;

fn alice_deposit_msg(context: &Context) -> DepositMsg {
    DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: None,
    }
}

fn withdraw_msg(utxo_storage_key: &str, output: Vec<TxOut>) -> TokenReceiverMessage {
    let utxo = utxo_storage_key.split('@').collect::<Vec<_>>();
    TokenReceiverMessage::Withdraw {
        target_btc_address: TARGET_ADDRESS.to_string(),
        input: vec![OutPoint {
            txid: utxo[0].parse().unwrap(),
            vout: utxo[1].parse().unwrap(),
        }],
        output,
        max_gas_fee: None,
        chain_specific_data: None,
    }
}

struct SignedWithdraw {
    btc_pending_id: String,
    withdraw_fee: u128,
    change_amount: u64,
}

async fn withdraw_and_sign(
    context: &Context,
    utxo_storage_key: &str,
    utxo_balance: u64,
    amount: u128,
) -> SignedWithdraw {
    let config = context.get_bridge_config().await.unwrap();
    let change_address = context.get_change_address().await.unwrap();
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(amount);
    let change_amount = utxo_balance - (amount - withdraw_fee) as u64;
    check!(context.do_withdraw(
        "alice",
        "bridge",
        amount,
        withdraw_msg(
            utxo_storage_key,
            vec![
                generate_tx_out(
                    (amount - BTC_GAS_FEE - withdraw_fee) as u64,
                    TARGET_ADDRESS,
                    Chain::BitcoinMainnet
                ),
                generate_tx_out(change_amount, &change_address, Chain::BitcoinMainnet),
            ]
        )
    ));
    let btc_pending_id = context
        .get_account("alice")
        .await
        .unwrap()
        .unwrap()
        .btc_pending_sign_ids
        .into_iter()
        .next()
        .unwrap();
    check!(context.sign_btc_transaction("relayer", &btc_pending_id, 0, 0));
    SignedWithdraw {
        btc_pending_id,
        withdraw_fee,
        change_amount,
    }
}

async fn setup_signed_withdraw(context: &Context) -> SignedWithdraw {
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    check!(context.set_btc_gas_fee_valid_range(10000, 200000));
    let alice_btc_deposit_address = context
        .get_user_deposit_address(alice_deposit_msg(context))
        .await
        .unwrap();
    check!(context.verify_deposit_v2(
        "relayer",
        alice_deposit_msg(context),
        generate_transaction_bytes(
            vec![(
                "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
                1,
                None,
            )],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (alice_btc_deposit_address.as_str(), DEPOSIT_AMOUNT),
            ],
        ),
        1,
        proof_json(BLOCKHASH.to_string(), 1, vec![])
    ));
    let utxo_storage_key = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    withdraw_and_sign(context, &utxo_storage_key, DEPOSIT_AMOUNT, WITHDRAW_AMOUNT).await
}

fn find_child(
    btc_pending_infos: &std::collections::HashMap<String, satoshi_bridge::BTCPendingInfo>,
    is_child: impl Fn(&PendingInfoState) -> bool,
) -> String {
    btc_pending_infos
        .iter()
        .find(|(_, v)| is_child(&v.state))
        .map(|(k, _)| k.clone())
        .expect("RBF child not found")
}

async fn finalize(context: &Context, winner_id: &str, original_id: &str) {
    check!(print "verify_withdraw_v2" context.verify_withdraw_v2(
        "relayer",
        winner_id,
        proof_json(BLOCKHASH.to_string(), 1, vec![])
    ));
    let btc_pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert!(!btc_pending_infos.contains_key(winner_id));
    assert!(!btc_pending_infos.contains_key(original_id));
}

async fn clear_orphan(context: &Context, child_id: &str) {
    context.get_btc_pending_infos_paged().await.unwrap()[child_id].assert_pending_sign();

    let outcome = context
        .clear_invalid_pending_verify_rbf("alice", child_id)
        .await;
    let err_msg = tool_err_msg(&outcome);
    assert!(
        err_msg.is_empty(),
        "clear_invalid_pending_verify_rbf failed: {err_msg}"
    );

    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    let alice = context.get_account("alice").await.unwrap().unwrap();
    assert!(alice.btc_pending_sign_ids.is_empty());
    assert!(alice.btc_pending_verify_list.is_empty());
}

async fn new_withdraw_after_cleanup(
    context: &Context,
    winner_id: &str,
    winner_change_amount: u64,
) -> SignedWithdraw {
    let change_utxo_storage_key = format!("{winner_id}@1");
    let change_utxo_balance =
        context.get_utxos_paged().await.unwrap()[&change_utxo_storage_key].balance;
    assert_eq!(change_utxo_balance, winner_change_amount);
    withdraw_and_sign(
        context,
        &change_utxo_storage_key,
        change_utxo_balance,
        NEW_WITHDRAW_AMOUNT,
    )
    .await
}

fn user_rbf_output(
    amount: u128,
    withdraw_fee: u128,
    gas_fee: u128,
    change_amount: u64,
    change_address: &str,
) -> Vec<TxOut> {
    vec![
        generate_tx_out(
            (amount - gas_fee - withdraw_fee) as u64,
            TARGET_ADDRESS,
            Chain::BitcoinMainnet,
        ),
        generate_tx_out(change_amount, change_address, Chain::BitcoinMainnet),
    ]
}

#[tokio::test]
async fn test_orphaned_user_rbf_child_cleanup() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let change_address = context.get_change_address().await.unwrap();
    let original = setup_signed_withdraw(&context).await;

    check!(context.withdraw_rbf(
        "alice",
        &original.btc_pending_id,
        user_rbf_output(
            WITHDRAW_AMOUNT,
            original.withdraw_fee,
            RBF_BTC_GAS_FEE,
            original.change_amount,
            &change_address,
        )
    ));
    let child_id = find_child(
        &context.get_btc_pending_infos_paged().await.unwrap(),
        |state| matches!(state, PendingInfoState::WithdrawUserRbf(_)),
    );

    finalize(&context, &original.btc_pending_id, &original.btc_pending_id).await;
    clear_orphan(&context, &child_id).await;

    let new_withdraw =
        new_withdraw_after_cleanup(&context, &original.btc_pending_id, original.change_amount)
            .await;
    check!(context.withdraw_rbf(
        "alice",
        &new_withdraw.btc_pending_id,
        user_rbf_output(
            NEW_WITHDRAW_AMOUNT,
            new_withdraw.withdraw_fee,
            RBF_BTC_GAS_FEE,
            new_withdraw.change_amount,
            &change_address,
        )
    ));
}

#[tokio::test]
async fn test_orphaned_cancel_withdraw_child_cleanup() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let change_address = context.get_change_address().await.unwrap();
    let original = setup_signed_withdraw(&context).await;

    check!(context.set_max_btc_tx_pending_sec(0));
    worker.fast_forward(2).await.unwrap();
    check!(context.cancel_withdraw(
        &original.btc_pending_id,
        vec![
            generate_tx_out(
                (WITHDRAW_AMOUNT - RBF_BTC_GAS_FEE - original.withdraw_fee) as u64,
                &change_address,
                Chain::BitcoinMainnet
            ),
            generate_tx_out(
                original.change_amount,
                &change_address,
                Chain::BitcoinMainnet
            ),
        ]
    ));
    let child_id = find_child(
        &context.get_btc_pending_infos_paged().await.unwrap(),
        |state| matches!(state, PendingInfoState::WithdrawCancelRbf(_)),
    );

    finalize(&context, &original.btc_pending_id, &original.btc_pending_id).await;
    clear_orphan(&context, &child_id).await;

    let new_withdraw =
        new_withdraw_after_cleanup(&context, &original.btc_pending_id, original.change_amount)
            .await;
    worker.fast_forward(2).await.unwrap();
    check!(context.cancel_withdraw(
        &new_withdraw.btc_pending_id,
        vec![
            generate_tx_out(
                (NEW_WITHDRAW_AMOUNT - RBF_BTC_GAS_FEE - new_withdraw.withdraw_fee) as u64,
                &change_address,
                Chain::BitcoinMainnet
            ),
            generate_tx_out(
                new_withdraw.change_amount,
                &change_address,
                Chain::BitcoinMainnet
            ),
        ]
    ));
}

#[tokio::test]
async fn test_rbf_child_cannot_be_cleared_while_original_pending() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let change_address = context.get_change_address().await.unwrap();
    let original = setup_signed_withdraw(&context).await;

    check!(context.withdraw_rbf(
        "alice",
        &original.btc_pending_id,
        user_rbf_output(
            WITHDRAW_AMOUNT,
            original.withdraw_fee,
            RBF_BTC_GAS_FEE,
            original.change_amount,
            &change_address,
        )
    ));
    let child_id = find_child(
        &context.get_btc_pending_infos_paged().await.unwrap(),
        |state| matches!(state, PendingInfoState::WithdrawUserRbf(_)),
    );

    let outcome = context
        .clear_invalid_pending_verify_rbf("alice", &child_id)
        .await;
    let err_msg = tool_err_msg(&outcome);
    println!("clear_invalid_pending_verify_rbf (original pending) errors: {err_msg}");
    assert!(!err_msg.is_empty());

    let btc_pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    btc_pending_infos[&original.btc_pending_id].assert_pending_verify();
    btc_pending_infos[&child_id].assert_pending_sign();
    let alice = context.get_account("alice").await.unwrap().unwrap();
    assert!(alice.btc_pending_sign_ids.contains(&child_id));
    assert!(alice
        .btc_pending_verify_list
        .contains(&original.btc_pending_id));
}

#[tokio::test]
async fn test_orphaned_rbf_child_cleanup_after_sibling_rbf_finalized() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let change_address = context.get_change_address().await.unwrap();
    let original = setup_signed_withdraw(&context).await;

    check!(context.withdraw_rbf(
        "alice",
        &original.btc_pending_id,
        user_rbf_output(
            WITHDRAW_AMOUNT,
            original.withdraw_fee,
            RBF_BTC_GAS_FEE,
            original.change_amount,
            &change_address,
        )
    ));
    let winner_id = find_child(
        &context.get_btc_pending_infos_paged().await.unwrap(),
        |state| matches!(state, PendingInfoState::WithdrawUserRbf(_)),
    );
    check!(context.sign_btc_transaction("relayer", &winner_id, 0, 0));

    check!(context.withdraw_rbf(
        "alice",
        &original.btc_pending_id,
        user_rbf_output(
            WITHDRAW_AMOUNT,
            original.withdraw_fee,
            RBF_BTC_GAS_FEE + 10000,
            original.change_amount,
            &change_address,
        )
    ));
    let child_id = context
        .get_account("alice")
        .await
        .unwrap()
        .unwrap()
        .btc_pending_sign_ids
        .into_iter()
        .next()
        .unwrap();
    assert_ne!(child_id, winner_id);
    context.get_btc_pending_infos_paged().await.unwrap()[&winner_id].assert_pending_verify();

    finalize(&context, &winner_id, &original.btc_pending_id).await;
    clear_orphan(&context, &child_id).await;

    new_withdraw_after_cleanup(&context, &winner_id, original.change_amount).await;
}
