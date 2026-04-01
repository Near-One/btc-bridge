mod setup;
use bitcoin::Transaction as BtcTransaction;
use near_sdk::serde_json::json;
use satoshi_bridge::DepositMsg;
use setup::*;

#[cfg(not(feature = "zcash"))]
const CHAIN: &str = "BitcoinMainnet";

#[cfg(not(feature = "zcash"))]
const TARGET_ADDRESS: &str = "1PAGsaT5vDz6hjzvuenSw33hWzESTR3ZHQ";

/// Helper: compute tx_id from tx_bytes (same as contract does)
fn compute_tx_id(tx_bytes: &[u8]) -> String {
    let tx: BtcTransaction = bitcoin::consensus::deserialize(tx_bytes).unwrap();
    tx.compute_txid().to_string()
}

/// Helper: build utxo_storage_key = "{tx_id}@{vout}"
fn utxo_storage_key(tx_bytes: &[u8], vout: u32) -> String {
    format!("{}@{}", compute_tx_id(tx_bytes), vout)
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_basic_flow() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let refund_btc_address = TARGET_ADDRESS;

    // 1. Get deposit address with refund_address set
    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").id().clone(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(refund_btc_address.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    assert!(!deposit_address.is_empty());

    // 2. Build a BTC transaction that sends to the deposit address
    let tx_bytes = generate_transaction_bytes(
        vec![(
            "a2a5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f19",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    // 3. Verify that UTXO is not yet known to the bridge
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 0);

    // 4. Request refund (anyone can call, proves tx via Light Client)
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![]
        )
    );

    // 5. Set timelock to 0 so execute works immediately
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "set_refund_timelock_sec")
        .args_json(json!({"refund_timelock_sec": 0}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // 6. Execute refund — should create BTCPendingInfo for sign pipeline
    let key = utxo_storage_key(&tx_bytes, vout);
    check!(
        print "execute_refund"
        context.execute_refund("alice", &key)
    );

    // 7. BTCPendingInfo should exist, pending sign
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending_infos.len(), 1);
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    let pending_values = pending_infos.values().cloned().collect::<Vec<_>>();
    pending_values[0].assert_pending_sign();

    // 8. Sign the refund transaction (1 input)
    check!(
        print "sign_btc_transaction"
        context.sign_btc_transaction("relayer", &pending_keys[0], 0, 0)
    );

    // 9. After signing all inputs, should transition to pending_verify
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_values = pending_infos.values().cloned().collect::<Vec<_>>();
    pending_values[0].assert_pending_verify();

    // 10. Refund request is gone (can't execute twice)
    check!(
        context.execute_refund("alice", &key),
        "Refund request not found"
    );

    // 11. No nBTC was minted
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_reject() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").id().clone(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "b3b5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f20",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 50_000)],
    );
    let vout: u32 = 0;

    // Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![]
        )
    );

    let key = utxo_storage_key(&tx_bytes, vout);

    // DAO rejects the refund
    check!(
        print "reject_refund"
        context.reject_refund("root", &key)
    );

    // Can't execute after rejection
    check!(
        context.execute_refund("alice", &key),
        "Refund request not found"
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_no_refund_address() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").id().clone(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: None,
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "c4c5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f21",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 50_000)],
    );

    // Should fail — no refund_address
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            tx_bytes,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![]
        ),
        "DepositMsg must contain refund_address"
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_duplicate_request() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").id().clone(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "d5d5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f22",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 50_000)],
    );

    // First request — should succeed
    check!(
        print "first request"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            tx_bytes.clone(),
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![]
        )
    );

    // Second request for same UTXO — should fail
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            tx_bytes,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![]
        ),
        "Refund request already exists for this UTXO"
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_then_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").id().clone(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    // Build BTC transaction to deposit address
    let tx_bytes = generate_transaction_bytes(
        vec![(
            "e6e6069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f23",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    // 1. Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![]
        )
    );

    // 2. Set timelock to 0 and execute refund
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "set_refund_timelock_sec")
        .args_json(json!({"refund_timelock_sec": 0}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    let key = utxo_storage_key(&tx_bytes, vout);
    check!(
        print "execute_refund"
        context.execute_refund("alice", &key)
    );

    // 3. Sign the refund transaction
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    check!(
        print "sign_btc_transaction"
        context.sign_btc_transaction("relayer", &pending_keys[0], 0, 0)
    );

    // 4. Now try verify_deposit with the same tx — should fail with "Already deposit utxo"
    check!(
        context.verify_deposit(
            "relayer",
            deposit_msg,
            tx_bytes,
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    // 5. No nBTC was minted
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}
