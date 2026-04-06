mod setup;
use bitcoin::{Amount, OutPoint, TxOut};
use near_sdk::{AccountId, Gas, NearToken};
use satoshi_bridge::network::{Address, Chain};
use satoshi_bridge::{DepositMsg, PendingInfoState, PostAction, TokenReceiverMessage};
use setup::*;
use std::string::ToString;

#[cfg(feature = "zcash")]
const CHAIN: &str = "ZcashTestnet";
#[cfg(not(feature = "zcash"))]
const CHAIN: &str = "BitcoinMainnet";

#[cfg(feature = "zcash")]
const TARGET_ADDRESS: &str = "tmD67UTsZ4iBbhCae4D43k1x8fhFNhwd4Jn";
#[cfg(not(feature = "zcash"))]
const TARGET_ADDRESS: &str = "1PAGsaT5vDz6hjzvuenSw33hWzESTR3ZHQ";

fn get_chain() -> Chain {
    match CHAIN {
        "ZcashTestnet" => Chain::ZcashTestnet,
        _ => Chain::BitcoinMainnet,
    }
}

#[tokio::test]
async fn test_role() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    assert_eq!(
        context.get_metadata().await.unwrap().super_admins,
        vec!["test.near".parse::<AccountId>().unwrap()]
    );
    check!(print context.bridge_add_super_admin("root", &context.get_account_by_name("alice").sdk_id()));
    assert_eq!(
        context.get_metadata().await.unwrap().super_admins,
        vec![
            "test.near".parse::<AccountId>().unwrap(),
            "alice.test.near".parse::<AccountId>().unwrap()
        ]
    );
    check!(print context.bridge_remove_super_admin("alice", &context.get_account_by_name("root").sdk_id()));
    assert_eq!(
        context.get_metadata().await.unwrap().super_admins,
        vec!["alice.test.near".parse::<AccountId>().unwrap()]
    );
    check!(
        context.bridge_add_super_admin("root", &context.get_account_by_name("alice").sdk_id()),
        "Insufficient permissions"
    );
    check!(
        context.bridge_remove_super_admin("alice", &context.get_account_by_name("alice").sdk_id()),
        "cannot remove oneself"
    );
    assert_eq!(
        context.get_metadata().await.unwrap().super_admins,
        vec!["alice.test.near".parse::<AccountId>().unwrap()]
    );
    // check!(printr context.bridge_acl_add_super_admin("alice", context.get_account_by_name("root").id()));
    // check!(view context.get_metadata());
    // check!(printr context.bridge_acl_grant_role("root", "DAO", context.get_account_by_name("root").id()));
    // check!(view context.get_metadata());
    // check!(view context.bridge_pa_all_paused());
    check!(print context.bridge_pa_pause_feature("alice", "ALL"));
    check!(
        context.verify_withdraw(
            "relayer",
            "",
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ),
        "Method is paused"
    );
    check!(
        context.verify_withdraw(
            "alice",
            "",
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ),
        "pending info not exist"
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .pa_all_paused
            .unwrap()
            .get("ALL"),
        Some(&"ALL".to_string())
    );
    check!(print context.bridge_pa_unpause_feature("alice", "ALL"));
    assert!(context
        .get_metadata()
        .await
        .unwrap()
        .pa_all_paused
        .is_none());
    check!(view context.get_metadata());
}

#[tokio::test]
async fn test_base() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    let bob_btc_deposit_address = context
        .get_user_deposit_address(DepositMsg {
            recipient_id: context.get_account_by_name("bob").sdk_id(),
            post_actions: None,
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        })
        .await
        .unwrap();
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
    check!(printr "alice 10000" context.verify_deposit(
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
            vec![
                (alice_btc_deposit_address.as_str(), 10000),
                (TARGET_ADDRESS, 90000)
            ],
        ),
        0,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 0);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 0);
    assert_eq!(
        context.get_unavailable_utxos_paged().await.unwrap().len(),
        1
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        0
    );
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 0);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        0
    );
    check!(printr "alice 50000" context.verify_deposit(
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
            ),],
            vec![
                (TARGET_ADDRESS, 90000),
                (alice_btc_deposit_address.as_str(), 50000),
            ],
        ),
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 50000);
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 0);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 1);
    assert_eq!(
        context.get_unavailable_utxos_paged().await.unwrap().len(),
        1
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        0
    );
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 0);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        0
    );
    check!(
        context.verify_deposit(
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
                ),],
                vec![
                    (TARGET_ADDRESS, 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 50000);
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 0);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 1);
    assert_eq!(
        context.get_unavailable_utxos_paged().await.unwrap().len(),
        1
    );
    check!(context.verify_deposit(
        "relayer",
        DepositMsg {
            recipient_id: context.get_account_by_name("bob").sdk_id(),
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
            ),],
            vec![
                (bob_btc_deposit_address.as_str(), 200000),
                (TARGET_ADDRESS, 50000),
            ],
        ),
        0,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 50000);
    assert_eq!(context.ft_balance_of("bob").await.unwrap().0, 200000);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 2);
    assert_eq!(
        context.get_unavailable_utxos_paged().await.unwrap().len(),
        1
    );
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 0);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        0
    );

    check!(context.ft_transfer("bob", "alice", 100000));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 150000);
    assert_eq!(context.ft_balance_of("bob").await.unwrap().0, 100000);

    check!(context.storage_deposit("nbtc", "bridge"));

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    let second_utxo = utxos_keys[1].split('@').collect::<Vec<_>>();
    let withdraw_amount = 110000;
    let btc_gas_fee = 25000;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let total_change_amount = 250000 - (withdraw_amount - withdraw_fee) as u64;
    check!(print context.do_withdraw("alice", "bridge", withdraw_amount, TokenReceiverMessage::Withdraw {
        target_btc_address: TARGET_ADDRESS.to_string(),
        input: vec![
            OutPoint {
            txid: first_utxo[0].parse().unwrap(),
            vout: first_utxo[1].parse().unwrap(),
        },
        OutPoint {
            txid: second_utxo[0].parse().unwrap(),
            vout: second_utxo[1].parse().unwrap(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat((withdraw_amount - btc_gas_fee - withdraw_fee) as u64),// 50000
            script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(total_change_amount / 4),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(total_change_amount / 4),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(total_change_amount / 4),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(total_change_amount / 4 + total_change_amount % 4),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        }],
        max_gas_fee: None,
        chain_specific_data: None,
    }));

    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        150000 - withdraw_amount
    ); //40000
    assert!(context.get_utxos_paged().await.unwrap().is_empty());

    assert!(context
        .get_account("alice")
        .await
        .unwrap()
        .unwrap()
        .btc_pending_sign_id
        .is_some());
    assert_eq!(
        context
            .get_account("alice")
            .await
            .unwrap()
            .unwrap()
            .btc_pending_verify_list
            .len(),
        0
    );
    let btc_pending_sign_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let keys = btc_pending_sign_txs.keys().cloned().collect::<Vec<_>>();
    let values = btc_pending_sign_txs.values().cloned().collect::<Vec<_>>();
    assert_eq!(btc_pending_sign_txs.len(), 1);
    values[0].assert_pending_sign();
    check!(print context.sign_btc_transaction("relayer", &keys[0], 0, 0));
    let btc_pending_sign_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let keys = btc_pending_sign_txs.keys().cloned().collect::<Vec<_>>();
    let values = btc_pending_sign_txs.values().cloned().collect::<Vec<_>>();
    values[0].assert_pending_sign();
    check!(
        context.sign_btc_transaction("relayer", &keys[0], 0, 0),
        "Already signed"
    );
    let btc_pending_sign_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let keys = btc_pending_sign_txs.keys().cloned().collect::<Vec<_>>();
    let values = btc_pending_sign_txs.values().cloned().collect::<Vec<_>>();
    values[0].assert_pending_sign();
    check!(print context.sign_btc_transaction("relayer", &keys[0], 1, 0));
    let btc_pending_sign_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let values = btc_pending_sign_txs.values().cloned().collect::<Vec<_>>();
    values[0].assert_pending_verify();
    assert!(context
        .get_account("alice")
        .await
        .unwrap()
        .unwrap()
        .btc_pending_sign_id
        .is_none());
    assert_eq!(
        context
            .get_account("alice")
            .await
            .unwrap()
            .unwrap()
            .btc_pending_verify_list
            .len(),
        1
    );
    assert_eq!(context.ft_total_supply().await.unwrap().0, 250000);
    let btc_pending_sign_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let keys = btc_pending_sign_txs.keys().cloned().collect::<Vec<_>>();
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 110000);
    check!(print context.verify_withdraw(
        "relayer",
        &keys[0],
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 5000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        45000
    );
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 45000);
    assert_eq!(
        250000 - (withdraw_amount - withdraw_fee), //withdraw_amount - withdraw_fee = 60000
        context.ft_total_supply().await.unwrap().0
    );
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    assert!(context
        .get_account("alice")
        .await
        .unwrap()
        .unwrap()
        .btc_pending_sign_id
        .is_none());
    assert_eq!(
        context
            .get_account("alice")
            .await
            .unwrap()
            .unwrap()
            .btc_pending_verify_list
            .len(),
        0
    );
}

#[tokio::test]
async fn test_fix_bridge_fee_and_relayer() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 1000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000
    );
    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    let withdraw_amount = 200000;
    let btc_gas_fee = 10000;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    check!(print "do_withdraw" context.do_withdraw("alice", "bridge", withdraw_amount, TokenReceiverMessage::Withdraw {
        target_btc_address: TARGET_ADDRESS.to_string(),
        input: vec![OutPoint {
            txid: first_utxo[0].parse().unwrap(),
            vout: first_utxo[1].parse().unwrap(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat((withdraw_amount - btc_gas_fee - withdraw_fee) as u64),// 50000
            script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(320000),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        }],
        max_gas_fee: None,
        chain_specific_data: None,
    }));
    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));
    let btc_pending_verify_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "verify_withdraw" context.verify_withdraw(
        "relayer",
        &btc_pending_verify_txs[0],
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(
        context.ft_balance_of("relayer").await.unwrap().0,
        1000 + 2000
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000 + 18000
    );
    check!(printr context.withdraw_protocol_fee(Some(9000)));
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000 + 18000
    );

    check!(context.storage_deposit("nbtc", "root"));
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        9000 + 18000
    );
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 0);
    check!(printr context.withdraw_protocol_fee(Some(9000)));
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 18000);
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 9000);
    check!(printr context.withdraw_protocol_fee(None));
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 0);
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 9000 + 18000);
}

#[tokio::test]
async fn test_ratio_bridge_fee_and_relayer() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(0, 1000, 9000));
    check!(context.set_withdraw_bridge_fee(0, 2000, 9000));
    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 5000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        45000
    );
    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    let withdraw_amount = 200000;
    let btc_gas_fee = 10000;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    check!(print "do_withdraw" context.do_withdraw("alice", "bridge", withdraw_amount, TokenReceiverMessage::Withdraw {
        target_btc_address: TARGET_ADDRESS.to_string(),
        input: vec![OutPoint {
            txid: first_utxo[0].parse().unwrap(),
            vout: first_utxo[1].parse().unwrap(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat((withdraw_amount - btc_gas_fee - withdraw_fee) as u64),// 50000
            script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(500000 - (withdraw_amount - withdraw_fee) as u64),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        }],
        max_gas_fee: None,
        chain_specific_data: None,
    }));
    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));
    let btc_pending_verify_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "verify_withdraw" context.verify_withdraw(
        "relayer",
        &btc_pending_verify_txs[0],
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(
        context.ft_balance_of("relayer").await.unwrap().0,
        5000 + 4000
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        45000 + 36000
    );
    check!(printr context.withdraw_protocol_fee(Some(9000)));
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        45000 + 36000
    );

    check!(context.storage_deposit("nbtc", "root"));
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        45000 + 36000
    );
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 0);
    check!(printr context.withdraw_protocol_fee(Some(45000)));
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 36000);
    assert_eq!(context.ft_balance_of("root").await.unwrap().0, 45000);
    check!(printr context.withdraw_protocol_fee(None));
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 0);
    assert_eq!(
        context.ft_balance_of("root").await.unwrap().0,
        45000 + 36000
    );
}

#[tokio::test]
async fn test_directly_withdraw() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 1000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000
    );

    assert_eq!(context.ft_balance_of("bob").await.unwrap().0, 0);
    check!(context.storage_deposit("nbtc", "bob"));
    check!(context.ft_transfer("alice", "bob", 200000));
    assert_eq!(context.ft_balance_of("bob").await.unwrap().0, 200000);

    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    let withdraw_amount = 200000;
    let btc_gas_fee = 10000;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    check!(print "do_withdraw" context.do_withdraw("bob", "bridge", withdraw_amount, TokenReceiverMessage::Withdraw {
        target_btc_address: TARGET_ADDRESS.to_string(),
        input: vec![OutPoint {
            txid: first_utxo[0].parse().unwrap(),
            vout: first_utxo[1].parse().unwrap(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat((withdraw_amount - btc_gas_fee - withdraw_fee) as u64),// 50000
            script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(320000),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        }],
        max_gas_fee: None,
        chain_specific_data: None,
    }));
    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));
    let btc_pending_verify_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "verify_withdraw" context.verify_withdraw(
        "relayer",
        &btc_pending_verify_txs[0],
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(
        context.ft_balance_of("relayer").await.unwrap().0,
        1000 + 2000
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000 + 18000
    );
}

#[tokio::test]
async fn test_one_click() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    let mut times = 0;
    {
        // dapp not in post_action_receiver_id_white_list
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![PostAction {
                receiver_id: context.get_account_by_name("dapp").sdk_id(),
                amount: 5000.into(),
                memo: None,
                msg: "".to_string(),
                gas: Some(Gas::from_tgas(100)),
            }]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));
        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times
        );
    }
    {
        // The account dapp.test.near is not registered，does not affect mint
        check!(
            context.extend_post_action_receiver_id_white_list(vec![context
                .get_account_by_name("dapp")
                .sdk_id()])
        );
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![PostAction {
                receiver_id: context.get_account_by_name("dapp").sdk_id(),
                amount: 5000.into(),
                memo: None,
                msg: "".to_string(),
                gas: Some(Gas::from_tgas(100)),
            }]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b1234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));

        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(context.ft_balance_of("dapp").await.unwrap().0, 0);
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times
        );
    }
    {
        // PostAction gas too large
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![PostAction {
                receiver_id: context.get_account_by_name("dapp").sdk_id(),
                amount: 5000.into(),
                memo: None,
                msg: "".to_string(),
                gas: Some(Gas::from_tgas(101)),
            }]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b1234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));

        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(context.ft_balance_of("dapp").await.unwrap().0, 0);
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times
        );
    }
    {
        // PostAction total gas too large
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 5000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(100)),
                },
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 5000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(40)),
                },
            ]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b1234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));

        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(context.ft_balance_of("dapp").await.unwrap().0, 0);
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times
        );
    }
    {
        // PostAction > 2
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 5000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: None,
                },
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 5000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: None,
                },
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 5000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: None,
                },
            ]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b1234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));

        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(context.ft_balance_of("dapp").await.unwrap().0, 0);
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times
        );
    }
    {
        // amount > current deposit
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![PostAction {
                receiver_id: context.get_account_by_name("dapp").sdk_id(),
                amount: 500000.into(),
                memo: None,
                msg: "".to_string(),
                gas: None,
            }]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b1234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));

        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(context.ft_balance_of("dapp").await.unwrap().0, 0);
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times
        );
    }
    {
        // The user is not registered with the dapp
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 20000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(50)),
                },
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 20000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(30)),
                },
            ]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b1234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));

        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(context.ft_balance_of("dapp").await.unwrap().0, 0);
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times
        );
    }
    {
        check!(context.storage_deposit("nbtc", "dapp"));
        check!(context.storage_deposit("dapp", "alice"));
        let deposit_msg = DepositMsg {
            recipient_id: context.get_account_by_name("alice").sdk_id(),
            post_actions: Some(vec![
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 20000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(100)),
                },
                PostAction {
                    receiver_id: context.get_account_by_name("dapp").sdk_id(),
                    amount: 20000.into(),
                    memo: None,
                    msg: "".to_string(),
                    gas: Some(Gas::from_tgas(30)),
                },
            ]),
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        };
        let alice_btc_deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        check!(printr "alice 50000" context.verify_deposit(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b1234",
                    1,
                    None,
                ),],
                vec![
                    ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                    (alice_btc_deposit_address.as_str(), 50000),
                ],
            ),
            1,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        ));

        times += 1;
        assert_eq!(
            context.ft_balance_of("relayer").await.unwrap().0,
            1000 * times
        );
        assert_eq!(
            context
                .get_metadata()
                .await
                .unwrap()
                .cur_available_protocol_fee,
            9000 * times
        );
        assert_eq!(context.ft_balance_of("dapp").await.unwrap().0, 40000);
        assert_eq!(
            context.ft_balance_of("alice").await.unwrap().0,
            (50000 - 10000) * times - 40000
        );
    }
}

#[tokio::test]
async fn test_utxo_passive_management() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(0, 0, 9000));
    check!(context.set_withdraw_bridge_fee(0, 0, 9000));
    // The bridge deposit fee is 0, so the bridge will not be automatically registered with mint
    check!(context.storage_deposit("nbtc", "bridge"));
    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);

    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    check!(printr "alice 60000" context.verify_deposit(
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
            ),],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (alice_btc_deposit_address.as_str(), 60000),
            ],
        ),
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 560000);

    let utxos = context.get_utxos_paged().await.unwrap();
    let utxo_key500000 = utxos
        .iter()
        .filter_map(|(k, v)| if v.balance == 500000 { Some(k) } else { None })
        .collect::<Vec<_>>()[0]
        .clone();
    let utxo500000 = utxo_key500000.split('@').collect::<Vec<_>>();
    let utxo_key60000 = utxos
        .iter()
        .filter_map(|(k, v)| if v.balance == 60000 { Some(k) } else { None })
        .collect::<Vec<_>>()[0]
        .clone();
    let utxo60000 = utxo_key60000.split('@').collect::<Vec<_>>();
    let withdraw_amount = 200000;
    let btc_gas_fee = 15000;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    check!(context.set_passive_management_limit(3, 10));
    check!(
        context.do_withdraw(
            "alice",
            "bridge",
            withdraw_amount,
            TokenReceiverMessage::Withdraw {
                target_btc_address: TARGET_ADDRESS.to_string(),
                input: vec![OutPoint {
                    txid: utxo500000[0].parse().unwrap(),
                    vout: utxo500000[1].parse().unwrap(),
                }],
                output: vec![
                    TxOut {
                        value: Amount::from_sat(
                            (withdraw_amount - btc_gas_fee - withdraw_fee) as u64
                        ),
                        script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
                            .expect("Invalid btc address")
                            .script_pubkey()
                            .expect("Failed to get script pubkey")
                    },
                    TxOut {
                        value: Amount::from_sat(500000 - (withdraw_amount - withdraw_fee) as u64),
                        script_pubkey: Address::parse(
                            withdraw_change_address.as_str(),
                            get_chain()
                        )
                        .expect("Invalid btc address")
                        .script_pubkey()
                        .expect("Failed to get script pubkey")
                    }
                ],
                max_gas_fee: None,
                chain_specific_data: None,
            }
        ),
        "require input_num < change_num"
    );
    check!(context.set_passive_management_limit(0, 1));
    let total_change = 500000 - (withdraw_amount - withdraw_fee) as u64;
    check!(
        context.do_withdraw(
            "alice",
            "bridge",
            withdraw_amount,
            TokenReceiverMessage::Withdraw {
                target_btc_address: TARGET_ADDRESS.to_string(),
                input: vec![OutPoint {
                    txid: utxo500000[0].parse().unwrap(),
                    vout: utxo500000[1].parse().unwrap(),
                }],
                output: vec![
                    TxOut {
                        value: Amount::from_sat(
                            (withdraw_amount - btc_gas_fee - withdraw_fee) as u64
                        ),
                        script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
                            .expect("Invalid btc address")
                            .script_pubkey()
                            .expect("Failed to get script pubkey")
                    },
                    TxOut {
                        value: Amount::from_sat(total_change / 2),
                        script_pubkey: Address::parse(
                            withdraw_change_address.as_str(),
                            get_chain()
                        )
                        .expect("Invalid btc address")
                        .script_pubkey()
                        .expect("Failed to get script pubkey")
                    },
                    TxOut {
                        value: Amount::from_sat(total_change / 2 + total_change % 2),
                        script_pubkey: Address::parse(
                            withdraw_change_address.as_str(),
                            get_chain()
                        )
                        .expect("Invalid btc address")
                        .script_pubkey()
                        .expect("Failed to get script pubkey")
                    }
                ],
                max_gas_fee: None,
                chain_specific_data: None,
            }
        ),
        "require input_num > change_num"
    );
    check!(context.set_passive_management_limit(0, u32::MAX));
    let total_change = 500000 + 60000 - (withdraw_amount - withdraw_fee) as u64;
    check!(
        context.do_withdraw(
            "alice",
            "bridge",
            withdraw_amount,
            TokenReceiverMessage::Withdraw {
                target_btc_address: TARGET_ADDRESS.to_string(),
                input: vec![
                    OutPoint {
                        txid: utxo500000[0].parse().unwrap(),
                        vout: utxo500000[1].parse().unwrap(),
                    },
                    OutPoint {
                        txid: utxo60000[0].parse().unwrap(),
                        vout: utxo60000[1].parse().unwrap(),
                    }
                ],
                output: vec![
                    TxOut {
                        value: Amount::from_sat(
                            (withdraw_amount - btc_gas_fee - withdraw_fee) as u64
                        ),
                        script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
                            .expect("Invalid btc address")
                            .script_pubkey()
                            .expect("Failed to get script pubkey")
                    },
                    TxOut {
                        value: Amount::from_sat(total_change),
                        script_pubkey: Address::parse(
                            withdraw_change_address.as_str(),
                            get_chain()
                        )
                        .expect("Invalid btc address")
                        .script_pubkey()
                        .expect("Failed to get script pubkey")
                    }
                ],
                max_gas_fee: None,
                chain_specific_data: None,
            }
        ),
        "The change amount must be less than all inputs"
    );
}

#[tokio::test]
async fn test_cancel_withdraw() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 1000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000
    );
    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    let withdraw_amount = 200000;
    let btc_gas_fee = 10000;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let change_amount = 500000 - (withdraw_amount - withdraw_fee) as u64;
    check!(print "do_withdraw" context.do_withdraw("alice", "bridge", withdraw_amount, TokenReceiverMessage::Withdraw {
        target_btc_address: TARGET_ADDRESS.to_string(),
        input: vec![OutPoint {
            txid: first_utxo[0].parse().unwrap(),
            vout: first_utxo[1].parse().unwrap(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat((withdraw_amount - btc_gas_fee - withdraw_fee) as u64),// 50000
            script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(change_amount),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        }],
        max_gas_fee: None,
        chain_specific_data: None,
    }));

    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));
    let original_btc_pending_verify_id = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>()[0]
        .clone();

    check!(
        context.cancel_withdraw(
            &original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    (withdraw_amount - btc_gas_fee - withdraw_fee) as u64,
                    TARGET_ADDRESS,
                    get_chain()
                ),
                generate_tx_out(change_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        ),
        "Please wait user rbf"
    );

    check!(context.set_max_btc_tx_pending_sec(0));

    check!(
        context.cancel_withdraw(
            &original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    (withdraw_amount - btc_gas_fee - withdraw_fee) as u64,
                    TARGET_ADDRESS,
                    get_chain()
                ),
                generate_tx_out(change_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        ),
        "Invalid output script_pubkey"
    );

    #[cfg(not(feature = "zcash"))]
    check!(
        context.cancel_withdraw(
            &original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    (withdraw_amount - btc_gas_fee - withdraw_fee) as u64,
                    withdraw_change_address.as_str(),
                    get_chain()
                ),
                generate_tx_out(change_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        ),
        "No gas increase."
    );

    let new_btc_gas_fee = 20000;
    check!(print
        context.cancel_withdraw(
            &original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    (withdraw_amount - new_btc_gas_fee - withdraw_fee) as u64,
                    withdraw_change_address.as_str(), get_chain()
                ),
                generate_tx_out(change_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        )
    );

    let btc_pending_verify_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let cancel_withdraw_tx_id = btc_pending_verify_txs
        .iter()
        .filter_map(|(k, v)| {
            if v.is_cancel_withdraw_rbf() {
                Some(k)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()[0]
        .clone();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &cancel_withdraw_tx_id, 0, 0));
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 1000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000
    );
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        500000 - 10000 - withdraw_amount
    );
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        9000 + withdraw_amount
    );
    assert_eq!(500000, context.ft_total_supply().await.unwrap().0);
    assert!(context.get_utxos_paged().await.unwrap().is_empty());
    assert!(context.get_btc_pending_infos_paged().await.unwrap().len() == 2);
    check!(print "verify_withdraw" context.verify_withdraw(
        "relayer",
        &cancel_withdraw_tx_id,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert!(context.get_utxos_paged().await.unwrap().len() == 2);
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        500000 - new_btc_gas_fee,
        context.ft_total_supply().await.unwrap().0
    );
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        500000 - 10000 - withdraw_amount + (withdraw_amount - withdraw_fee - new_btc_gas_fee)
    );
    assert_eq!(
        context.ft_balance_of("relayer").await.unwrap().0,
        1000 + 2000
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000 + 18000
    );
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        9000 + 18000
    );
}

#[tokio::test]
async fn test_cancel_withdraw2() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 9000));
    check!(context.set_withdraw_bridge_fee(20000, 0, 9000));
    let config = context.get_bridge_config().await.unwrap();
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 1000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000
    );
    let utxos_keys = context
        .get_utxos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    let first_utxo = utxos_keys[0].split('@').collect::<Vec<_>>();
    let withdraw_amount = 200000;
    let btc_gas_fee = 10000;
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount);
    let change_amount = 500000 - (withdraw_amount - withdraw_fee) as u64;
    check!(print "do_withdraw" context.do_withdraw("alice", "bridge", withdraw_amount, TokenReceiverMessage::Withdraw {
        target_btc_address: TARGET_ADDRESS.to_string(),
        input: vec![OutPoint {
            txid: first_utxo[0].parse().unwrap(),
            vout: first_utxo[1].parse().unwrap(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat((withdraw_amount - btc_gas_fee - withdraw_fee) as u64),// 50000
            script_pubkey: Address::parse(TARGET_ADDRESS, get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        },TxOut {
            value: Amount::from_sat(change_amount),
            script_pubkey: Address::parse(withdraw_change_address.as_str(), get_chain())
            .expect("Invalid btc address")
            .script_pubkey().expect("Failed to get script pubkey")
        }],
        max_gas_fee: None,
        chain_specific_data: None,
    }));

    let btc_pending_sign_txs = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &btc_pending_sign_txs[0], 0, 0));
    let original_btc_pending_verify_id = btc_pending_sign_txs[0].clone();

    check!(context.set_max_btc_tx_pending_sec(0));
    check!(context.set_btc_gas_fee_valid_range(10000, 200000));
    // let new_btc_gas_fee = 200000;
    check!(print
        context.cancel_withdraw(
            &original_btc_pending_verify_id,
            vec![
                // generate_tx_out(
                //     (withdraw_amount - new_btc_gas_fee - withdraw_fee) as u64,
                //     withdraw_change_address.as_str()
                // ),
                generate_tx_out(change_amount - 111, withdraw_change_address.as_str(), get_chain()),
            ]
        )
    );

    let btc_pending_verify_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let cancel_withdraw_tx_id = btc_pending_verify_txs
        .iter()
        .filter_map(|(k, v)| {
            if v.is_cancel_withdraw_rbf() {
                Some(k)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()[0]
        .clone();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &cancel_withdraw_tx_id, 0, 0));
    assert_eq!(context.ft_balance_of("relayer").await.unwrap().0, 1000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000 - 111
    );
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        500000 - 10000 - withdraw_amount
    );
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        9000 + withdraw_amount
    );
    assert_eq!(500000, context.ft_total_supply().await.unwrap().0);
    assert!(context.get_utxos_paged().await.unwrap().is_empty());
    assert!(context.get_btc_pending_infos_paged().await.unwrap().len() == 2);
    check!(print context.verify_withdraw(
        "relayer",
        &original_btc_pending_verify_id,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert!(context.get_btc_pending_infos_paged().await.unwrap().len() == 1);
    assert_eq!(
        500000 - (withdraw_amount - withdraw_fee),
        context.ft_total_supply().await.unwrap().0
    );
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        500000 - 10000 - withdraw_amount
    );
    assert_eq!(
        context.ft_balance_of("relayer").await.unwrap().0,
        1000 + 2000
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        9000 + 18000
    );
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        9000 + 18000
    );
    check!(context.clear_invalid_pending_verify_rbf("relayer", &cancel_withdraw_tx_id));

    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn test_utxo_active_management() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 10000));
    // The bridge deposit fee is 0, so the bridge will not be automatically registered with mint
    check!(context.storage_deposit("nbtc", "bridge"));
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);

    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    check!(printr "alice 60000" context.verify_deposit(
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
            ),],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (alice_btc_deposit_address.as_str(), 60000),
            ],
        ),
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        560000 - 20000
    );

    let utxos = context.get_utxos_paged().await.unwrap();
    let utxo_key500000 = utxos
        .iter()
        .filter_map(|(k, v)| if v.balance == 500000 { Some(k) } else { None })
        .collect::<Vec<_>>()[0]
        .clone();
    let utxo500000 = utxo_key500000.split('@').collect::<Vec<_>>();
    let utxo_key60000 = utxos
        .iter()
        .filter_map(|(k, v)| if v.balance == 60000 { Some(k) } else { None })
        .collect::<Vec<_>>()[0]
        .clone();
    let utxo60000 = utxo_key60000.split('@').collect::<Vec<_>>();
    let output_amount = 560000 / 2;
    check!(
        context.active_utxo_management(
            vec![
                OutPoint {
                    txid: utxo500000[0].parse().unwrap(),
                    vout: utxo500000[1].parse().unwrap(),
                },
                OutPoint {
                    txid: utxo60000[0].parse().unwrap(),
                    vout: utxo60000[1].parse().unwrap(),
                }
            ],
            vec![
                generate_tx_out(output_amount, TARGET_ADDRESS, get_chain()),
                generate_tx_out(output_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        ),
        "Active management conditions are not met"
    );
    check!(context.set_active_management_limit(3, 10));
    check!(
        context.active_utxo_management(
            vec![
                OutPoint {
                    txid: utxo500000[0].parse().unwrap(),
                    vout: utxo500000[1].parse().unwrap(),
                },
                OutPoint {
                    txid: utxo60000[0].parse().unwrap(),
                    vout: utxo60000[1].parse().unwrap(),
                }
            ],
            vec![
                generate_tx_out(output_amount, TARGET_ADDRESS, get_chain()),
                generate_tx_out(output_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        ),
        "require input_num < output_num"
    );
    check!(context.set_active_management_limit(0, 1));
    check!(
        context.active_utxo_management(
            vec![
                OutPoint {
                    txid: utxo500000[0].parse().unwrap(),
                    vout: utxo500000[1].parse().unwrap(),
                },
                OutPoint {
                    txid: utxo60000[0].parse().unwrap(),
                    vout: utxo60000[1].parse().unwrap(),
                }
            ],
            vec![
                generate_tx_out(output_amount, TARGET_ADDRESS, get_chain()),
                generate_tx_out(output_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        ),
        "require input_num > output_num"
    );
    check!(
        context.active_utxo_management(
            vec![
                OutPoint {
                    txid: utxo500000[0].parse().unwrap(),
                    vout: utxo500000[1].parse().unwrap(),
                },
                OutPoint {
                    txid: utxo60000[0].parse().unwrap(),
                    vout: utxo60000[1].parse().unwrap(),
                }
            ],
            vec![generate_tx_out(
                output_amount * 2,
                TARGET_ADDRESS,
                get_chain()
            ),]
        ),
        "Invalid output script_pubkey"
    );
    check!(
        context.active_utxo_management(
            vec![
                OutPoint {
                    txid: utxo500000[0].parse().unwrap(),
                    vout: utxo500000[1].parse().unwrap(),
                },
                OutPoint {
                    txid: utxo60000[0].parse().unwrap(),
                    vout: utxo60000[1].parse().unwrap(),
                }
            ],
            vec![generate_tx_out(
                output_amount * 2 - 30000,
                withdraw_change_address.as_str(),
                get_chain()
            ),]
        ),
        "Insufficient protocol_fee"
    );
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        20000
    );
    check!(print
        context.active_utxo_management(
            vec![OutPoint {
                txid: utxo500000[0].parse().unwrap(),
                vout: utxo500000[1].parse().unwrap(),
            },OutPoint {
                txid: utxo60000[0].parse().unwrap(),
                vout: utxo60000[1].parse().unwrap(),
            }],
            vec![
                generate_tx_out(
                    output_amount * 2 - 10000,
                    withdraw_change_address.as_str(), get_chain()
                ),
            ]
        )
    );
    let btc_pending_sign_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let original_btc_pending_verify_id = btc_pending_sign_txs.keys().collect::<Vec<_>>()[0];
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", original_btc_pending_verify_id, 0, 0));
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", original_btc_pending_verify_id, 1, 0));
    check!(
        context.active_utxo_management_rbf(
            original_btc_pending_verify_id,
            vec![generate_tx_out(
                output_amount * 2 - 10000,
                withdraw_change_address.as_str(),
                get_chain()
            ),]
        ),
        "No gas increase."
    );
    check!(
        context.active_utxo_management_rbf(
            original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    output_amount - 10000,
                    withdraw_change_address.as_str(),
                    get_chain()
                ),
                generate_tx_out(
                    output_amount - 10000,
                    withdraw_change_address.as_str(),
                    get_chain()
                ),
            ]
        ),
        "Invalid output num"
    );
    check!(
        context.active_utxo_management_rbf(
            original_btc_pending_verify_id,
            vec![generate_tx_out(
                output_amount * 2 - 25000,
                withdraw_change_address.as_str(),
                get_chain()
            ),]
        ),
        "Insufficient protocol fee"
    );
    check!(context.active_utxo_management_rbf(
        original_btc_pending_verify_id,
        vec![generate_tx_out(
            output_amount * 2 - 15000,
            withdraw_change_address.as_str(),
            get_chain()
        ),]
    ));

    let btc_pending_verify_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let active_utxo_management_rbf_id = btc_pending_verify_txs
        .iter()
        .filter_map(|(k, v)| {
            if matches!(v.state, PendingInfoState::ActiveUtxoManagementRbf(..)) {
                Some(k)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()[0]
        .clone();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &active_utxo_management_rbf_id, 0, 0));
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &active_utxo_management_rbf_id, 1, 0));
    check!(
        context.cancel_active_utxo_management(
            original_btc_pending_verify_id,
            vec![generate_tx_out(
                output_amount * 2 - 15000,
                withdraw_change_address.as_str(),
                get_chain()
            ),]
        ),
        "Please wait user rbf"
    );
    check!(context.set_max_btc_tx_pending_sec(0));
    check!(
        context.cancel_active_utxo_management(
            original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    output_amount - 15000,
                    withdraw_change_address.as_str(),
                    get_chain()
                ),
                generate_tx_out(output_amount, withdraw_change_address.as_str(), get_chain()),
            ]
        ),
        "No gas increase."
    );
    check!(print
        context.cancel_active_utxo_management(
            original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    output_amount - 16000,
                    withdraw_change_address.as_str(), get_chain()
                ),
                generate_tx_out(
                    output_amount,
                    withdraw_change_address.as_str(), get_chain()
                ),
            ]
        )
    );
    let btc_pending_verify_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let cancel_active_utxo_management_tx_id = btc_pending_verify_txs
        .iter()
        .filter_map(|(k, v)| {
            if matches!(v.state, PendingInfoState::ActiveUtxoManagementCancelRbf(..)) {
                Some(k)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()[0]
        .clone();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &cancel_active_utxo_management_tx_id, 0, 0));
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &cancel_active_utxo_management_tx_id, 1, 0));

    assert_eq!(560000, context.ft_total_supply().await.unwrap().0);
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 20000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        20000 - 16000
    );
    check!(print context.verify_active_utxo_management(
        "relayer",
        &cancel_active_utxo_management_tx_id,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        20000 - 16000
    );
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        20000 - 16000,
    );
    assert_eq!(560000 - 16000, context.ft_total_supply().await.unwrap().0);
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        560000 - 20000
    );
}

#[tokio::test]
async fn test_utxo_active_management2() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    check!(context.set_deposit_bridge_fee(10000, 0, 10000));
    // The bridge deposit fee is 0, so the bridge will not be automatically registered with mint
    check!(context.storage_deposit("nbtc", "bridge"));
    let withdraw_change_address = context.get_change_address().await.unwrap();
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
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);

    check!(printr "alice 500000" context.verify_deposit(
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
            ),],
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
    check!(printr "alice 60000" context.verify_deposit(
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
            ),],
            vec![
                ("1MgiBKohM2poApYamQadp21vJrNyh5T19G", 90000),
                (alice_btc_deposit_address.as_str(), 60000),
            ],
        ),
        1,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        560000 - 20000
    );

    let utxos = context.get_utxos_paged().await.unwrap();
    let utxo_key500000 = utxos
        .iter()
        .filter_map(|(k, v)| if v.balance == 500000 { Some(k) } else { None })
        .collect::<Vec<_>>()[0]
        .clone();
    let utxo500000 = utxo_key500000.split('@').collect::<Vec<_>>();
    let utxo_key60000 = utxos
        .iter()
        .filter_map(|(k, v)| if v.balance == 60000 { Some(k) } else { None })
        .collect::<Vec<_>>()[0]
        .clone();
    let utxo60000 = utxo_key60000.split('@').collect::<Vec<_>>();
    let output_amount = 560000 / 2;
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        20000
    );
    check!(context.set_active_management_limit(0, 1));
    check!(print
        context.active_utxo_management(
            vec![OutPoint {
                txid: utxo500000[0].parse().unwrap(),
                vout: utxo500000[1].parse().unwrap(),
            },OutPoint {
                txid: utxo60000[0].parse().unwrap(),
                vout: utxo60000[1].parse().unwrap(),
            }],
            vec![
                generate_tx_out(
                    output_amount * 2 - 10000,
                    withdraw_change_address.as_str(), get_chain()
                ),
            ]
        )
    );
    let btc_pending_sign_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let original_btc_pending_verify_id = btc_pending_sign_txs.keys().collect::<Vec<_>>()[0];
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", original_btc_pending_verify_id, 0, 0));
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", original_btc_pending_verify_id, 1, 0));
    check!(context.active_utxo_management_rbf(
        original_btc_pending_verify_id,
        vec![generate_tx_out(
            output_amount * 2 - 15000,
            withdraw_change_address.as_str(),
            get_chain()
        ),]
    ));
    let btc_pending_verify_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let active_utxo_management_rbf_tx_id = btc_pending_verify_txs
        .iter()
        .filter_map(|(k, v)| {
            if matches!(v.state, PendingInfoState::ActiveUtxoManagementRbf(..)) {
                Some(k)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()[0]
        .clone();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &active_utxo_management_rbf_tx_id, 0, 0));
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &active_utxo_management_rbf_tx_id, 1, 0));
    check!(context.set_max_btc_tx_pending_sec(0));
    check!(print
        context.cancel_active_utxo_management(
            original_btc_pending_verify_id,
            vec![
                generate_tx_out(
                    output_amount - 16000,
                    withdraw_change_address.as_str(), get_chain()
                ),
                generate_tx_out(
                    output_amount,
                    withdraw_change_address.as_str(), get_chain()
                ),
            ]
        )
    );
    let btc_pending_verify_txs = context.get_btc_pending_infos_paged().await.unwrap();
    let cancel_active_utxo_management_tx_id = btc_pending_verify_txs
        .iter()
        .filter_map(|(k, v)| {
            if matches!(v.state, PendingInfoState::ActiveUtxoManagementCancelRbf(..)) {
                Some(k)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()[0]
        .clone();
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &cancel_active_utxo_management_tx_id, 0, 0));
    check!(print "sign_btc_transaction" context.sign_btc_transaction("relayer", &cancel_active_utxo_management_tx_id, 1, 0));
    assert_eq!(560000, context.ft_total_supply().await.unwrap().0);
    assert_eq!(context.ft_balance_of("bridge").await.unwrap().0, 20000);
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        20000 - 16000
    );
    check!(print context.verify_active_utxo_management(
        "relayer",
        &active_utxo_management_rbf_tx_id,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![]
    ));
    assert_eq!(
        context
            .get_metadata()
            .await
            .unwrap()
            .cur_available_protocol_fee,
        20000 - 15000
    );
    assert_eq!(
        context.ft_balance_of("bridge").await.unwrap().0,
        20000 - 15000,
    );
    assert_eq!(560000 - 15000, context.ft_total_supply().await.unwrap().0);
    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        560000 - 20000
    );
}

#[tokio::test]
async fn test_unauthorized_account_cannot_call_trusted_relayer_methods() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    // Create a new account that does NOT receive the UnrestrictedRelayer role.
    // Context::new only grants UnrestrictedRelayer to relayer, alice, bob, charlie, and tx_listener.
    let unauthorized = worker.dev_create_account().await.unwrap();

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

    // verify_deposit should fail for an account without the trusted-relayer role
    let outcome = unauthorized
        .call(context.bridge_contract.id(), "verify_deposit")
        .args_json(near_sdk::serde_json::json!({
            "deposit_msg": DepositMsg {
                recipient_id: context.get_account_by_name("alice").sdk_id(),
                post_actions: None,
                extra_msg: None,
                safe_deposit: None,
                refund_address: None,
            },
            "tx_bytes": generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
                    1,
                    None,
                )],
                vec![
                    (alice_btc_deposit_address.as_str(), 50000),
                    (TARGET_ADDRESS, 90000),
                ],
            ),
            "vout": 0u32,
            "tx_block_blockhash": "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d",
            "tx_index": 1u64,
            "merkle_proof": Vec::<String>::new(),
        }))
        .max_gas()
        .transact()
        .await;
    assert!(
        tool_err_msg(&outcome).contains("Relayer is not active"),
        "verify_deposit should reject an account without trusted-relayer role"
    );

    // safe_verify_deposit should fail for an account without the trusted-relayer role
    let outcome = unauthorized
        .call(context.bridge_contract.id(), "safe_verify_deposit")
        .args_json(near_sdk::serde_json::json!({
            "deposit_msg": DepositMsg {
                recipient_id: context.get_account_by_name("alice").sdk_id(),
                post_actions: None,
                extra_msg: None,
                safe_deposit: None,
                refund_address: None,
            },
            "tx_bytes": generate_transaction_bytes(
                vec![(
                    "c6774e76452c36bba6c357653f620a4364fc063ba021e2acf6049f8d9e6b0234",
                    1,
                    None,
                )],
                vec![
                    (alice_btc_deposit_address.as_str(), 50000),
                    (TARGET_ADDRESS, 90000),
                ],
            ),
            "vout": 0u32,
            "tx_block_blockhash": "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d",
            "tx_index": 1u64,
            "merkle_proof": Vec::<String>::new(),
        }))
        .max_gas()
        .deposit(NearToken::from_near(1))
        .transact()
        .await;
    assert!(
        tool_err_msg(&outcome).contains("Relayer is not active"),
        "safe_verify_deposit should reject an account without trusted-relayer role"
    );

    // verify_withdraw should fail for an account without the trusted-relayer role
    let outcome = unauthorized
        .call(context.bridge_contract.id(), "verify_withdraw")
        .args_json(near_sdk::serde_json::json!({
            "tx_id": "",
            "tx_block_blockhash": "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d",
            "tx_index": 1u64,
            "merkle_proof": Vec::<String>::new(),
        }))
        .max_gas()
        .transact()
        .await;
    assert!(
        tool_err_msg(&outcome).contains("Relayer is not active"),
        "verify_withdraw should reject an account without trusted-relayer role"
    );

    // verify_active_utxo_management should fail for an account without the trusted-relayer role
    let outcome = unauthorized
        .call(
            context.bridge_contract.id(),
            "verify_active_utxo_management",
        )
        .args_json(near_sdk::serde_json::json!({
            "tx_id": "",
            "tx_block_blockhash": "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d",
            "tx_index": 1u64,
            "merkle_proof": Vec::<String>::new(),
        }))
        .max_gas()
        .transact()
        .await;
    assert!(
        tool_err_msg(&outcome).contains("Relayer is not active"),
        "verify_active_utxo_management should reject an account without trusted-relayer role"
    );
}
