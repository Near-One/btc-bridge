#![cfg(not(feature = "zcash"))]

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
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![],
            None
        )
    );

    let key = utxo_storage_key(&tx_bytes, vout);

    // 5. Set timelock to 200 seconds
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 200}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // 6. Execute immediately — should fail (timelock not passed)
    check!(
        context.execute_refund("alice", &key),
        "Refund timelock has not passed yet"
    );

    // 7. Fast-forward past timelock (3600 seconds = ~3600 blocks)
    worker.fast_forward(4000).await.unwrap();

    // 8. Execute refund — timelock passed, should succeed

    let storage_before = context
        .bridge_contract
        .view_account()
        .await
        .unwrap()
        .storage_usage;

    check!(
        print "execute_refund"
        context.execute_refund("alice", &key)
    );

    let storage_after = context
        .bridge_contract
        .view_account()
        .await
        .unwrap()
        .storage_usage;

    let storage_used = storage_after - storage_before;
    let cost_per_byte = 10u128.pow(19); // 0.00001 NEAR per byte
    let storage_cost_yocto = storage_used as u128 * cost_per_byte;
    println!("==> Storage used by execute_refund: {} bytes", storage_used);
    println!("==> Storage cost: {} yoctoNEAR", storage_cost_yocto);
    println!(
        "==> Storage cost: {:.4} NEAR",
        storage_cost_yocto as f64 / 1e24
    );

    // Verify that required_balance_for_execute_refund covers actual storage cost
    let required_balance = context.required_balance_for_execute_refund().await.unwrap();
    println!(
        "==> required_balance_for_execute_refund: {} yoctoNEAR ({:.4} NEAR)",
        required_balance.as_yoctonear(),
        required_balance.as_yoctonear() as f64 / 1e24
    );
    assert!(
        required_balance.as_yoctonear() >= storage_cost_yocto,
        "required_balance_for_execute_refund ({}) is less than actual storage cost ({})",
        required_balance.as_yoctonear(),
        storage_cost_yocto,
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
        context.sign_btc_transaction("alice", &pending_keys[0], 0, 0)
    );

    // 9. After signing all inputs, should transition to pending_verify
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_values = pending_infos.values().cloned().collect::<Vec<_>>();
    pending_values[0].assert_pending_verify();

    // 10. Verify refund transaction on-chain (like verify_withdraw but no burn)
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    check!(
        print "verify_refund_finalize"
        context.verify_refund_finalize(
            "relayer",
            &pending_keys[0],
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![]
        )
    );

    // 11. Pending info cleaned up
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());

    // 12. Refund request is gone (can't execute twice)
    check!(
        context.execute_refund("alice", &key),
        "Refund request not found"
    );

    // 13. No nBTC was minted
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_reject() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![],
            None
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
    // refund_address in DepositMsg is optional — refund_address is now a separate parameter.
    // This test verifies that request_refund succeeds even when DepositMsg has no refund_address.
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

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

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "c4c5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f21",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 50_000)],
    );

    // Should succeed — refund_address provided as separate parameter
    check!(
        print "request_refund_no_addr_in_msg"
        context.request_refund(
            "alice",
            deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
            None
        )
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_duplicate_request() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
        vec![(deposit_address.as_str(), 100_000)],
    );

    // First request — should succeed
    check!(
        print "first request"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            tx_bytes.clone(),
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![],
            None
        )
    );

    // Second request for same UTXO — should fail
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
            None
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
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "e6e6069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f23",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // 1. Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![],
            None
        )
    );

    // 2. Set timelock to 0 and execute refund
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 0}}))
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

    // 3. verify_deposit blocked RIGHT AFTER execute_refund (before sign)
    check!(
        context.verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    // 4. Sign the refund transaction
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    check!(
        print "sign_btc_transaction"
        context.sign_btc_transaction("alice", &pending_keys[0], 0, 0)
    );

    // 5. verify_deposit STILL blocked after sign (after broadcast)
    check!(
        context.verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    // 6. verify_refund_finalize — finalize the refund
    check!(
        print "verify_refund_finalize"
        context.verify_refund_finalize(
            "relayer",
            &pending_keys[0],
            blockhash.clone(),
            1,
            vec![]
        )
    );

    // 7. Pending info cleaned up
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());

    // 8. verify_deposit STILL blocked after verify_refund_finalize
    check!(
        context.verify_deposit("relayer", deposit_msg, tx_bytes, vout, blockhash, 1, vec![]),
        "Already deposit utxo"
    );

    // 9. No nBTC was minted
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_race_deposit_wins() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "f7f7069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f24",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // 1. Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![],
            None
        )
    );

    // 2. During timelock, Relayer calls verify_deposit — deposit succeeds
    check!(
        print "verify_deposit"
        context.verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        )
    );

    // 3. nBTC minted to alice
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);

    // 4. Set timelock to 0
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 0}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // 5. execute_refund fails — UTXO already verified via deposit
    let key = utxo_storage_key(&tx_bytes, vout);
    check!(
        context.execute_refund("alice", &key),
        "UTXO already verified via deposit, cannot refund"
    );

    // 6. nBTC still there — deposit was the winner
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_after_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "a8a8069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f25",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // 1. verify_deposit — Relayer finalizes deposit first
    check!(
        print "verify_deposit"
        context.verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        )
    );

    // 2. nBTC minted to alice
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);

    // 3. request_refund fails — UTXO already verified via deposit
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            vout,
            blockhash,
            1,
            vec![],
            None
        ),
        "UTXO already verified via deposit"
    );

    // 4. nBTC still there
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_reject_then_deposit_succeeds() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "b9b9069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f26",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // 1. Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![],
            None
        )
    );

    let key = utxo_storage_key(&tx_bytes, vout);

    // 2. DAO rejects the refund
    check!(
        print "reject_refund"
        context.reject_refund("root", &key)
    );

    // 3. execute_refund fails — request was rejected
    check!(
        context.execute_refund("alice", &key),
        "Refund request not found"
    );

    // 4. verify_deposit works normally — UTXO was not marked
    check!(
        print "verify_deposit"
        context.verify_deposit(
            "relayer",
            deposit_msg,
            tx_bytes,
            vout,
            blockhash,
            1,
            vec![]
        )
    );

    // 5. nBTC minted to alice
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 100_000);
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_double_request_after_execute() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "caca069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f27",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // 1. Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![],
            None
        )
    );

    // 2. Set timelock to 0 and execute refund
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 0}}))
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

    // 3. Second request_refund — should fail (UTXO marked in verified_deposit_utxo)
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            vout,
            blockhash,
            1,
            vec![],
            None
        ),
        "UTXO already verified via deposit"
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_spoofed_refund_address() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    // Alice creates a real deposit with her refund address
    let real_deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(real_deposit_msg.clone())
        .await
        .unwrap();

    // BTC transaction sends to the real deposit address
    let tx_bytes = generate_transaction_bytes(
        vec![(
            "dbdb069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f28",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // Attacker creates a spoofed deposit_msg with a DIFFERENT refund_address
    // but same recipient_id — this changes the hash, so script_pubkey won't match
    let spoofed_deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string()), // attacker's address
    };

    // request_refund with spoofed deposit_msg — callback should panic
    // because script_pubkey derived from spoofed msg won't match tx output
    check!(
        context.request_refund(
            "bob",
            spoofed_deposit_msg,
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![],
            None
        ),
        "refund_address does not match deposit_msg.refund_address"
    );

    // Real request_refund still works
    check!(
        print "real request_refund"
        context.request_refund(
            "alice",
            real_deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            vout,
            blockhash,
            1,
            vec![],
            None
        )
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_race_safe_deposit_wins() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: Some(satoshi_bridge::SafeDepositMsg {
            msg: "".to_string(),
        }),
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "ecec069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f29",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // 1. Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![],
            None
        )
    );

    // 2. During timelock, Relayer calls safe_verify_deposit — succeeds
    // Register alice in nBTC first (safe_mint requires it)
    check!(context.storage_deposit("nbtc", "alice"));
    check!(
        print "safe_verify_deposit"
        context.safe_verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        )
    );

    // 3. nBTC minted to alice
    assert!(context.ft_balance_of("alice").await.unwrap().0 > 0);

    // 4. Set timelock to 0
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 0}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // 5. execute_refund fails — UTXO already verified via safe deposit
    let key = utxo_storage_key(&tx_bytes, vout);
    check!(
        context.execute_refund("alice", &key),
        "UTXO already verified via deposit, cannot refund"
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_after_safe_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: Some(satoshi_bridge::SafeDepositMsg {
            msg: "".to_string(),
        }),
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "fdfd069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f30",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // 1. Register alice in nBTC and do safe_verify_deposit
    check!(context.storage_deposit("nbtc", "alice"));
    check!(
        print "safe_verify_deposit"
        context.safe_verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        )
    );

    // 2. nBTC minted
    assert!(context.ft_balance_of("alice").await.unwrap().0 > 0);

    // 3. request_refund fails — UTXO already verified
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            vout,
            blockhash,
            1,
            vec![],
            None
        ),
        "UTXO already verified via deposit"
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_then_safe_deposit_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: Some(satoshi_bridge::SafeDepositMsg {
            msg: "".to_string(),
        }),
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };

    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "abab069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f31",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let blockhash = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string();

    // Register alice in nBTC (needed for safe_verify_deposit attempts)
    check!(context.storage_deposit("nbtc", "alice"));

    // 1. Request refund
    check!(
        print "request_refund"
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![],
            None
        )
    );

    // 2. Set timelock to 0 and execute refund
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 0}}))
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

    // 3. safe_verify_deposit blocked RIGHT AFTER execute_refund (before sign)
    check!(
        context.safe_verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    // 4. Sign the refund transaction
    let pending_infos = context.get_btc_pending_infos_paged().await.unwrap();
    let pending_keys = pending_infos.keys().cloned().collect::<Vec<_>>();
    check!(
        print "sign_btc_transaction"
        context.sign_btc_transaction("alice", &pending_keys[0], 0, 0)
    );

    // 5. safe_verify_deposit STILL blocked after sign (after broadcast)
    check!(
        context.safe_verify_deposit(
            "relayer",
            deposit_msg.clone(),
            tx_bytes.clone(),
            vout,
            blockhash.clone(),
            1,
            vec![]
        ),
        "Already deposit utxo"
    );

    // 6. verify_refund_finalize — finalize the refund
    check!(
        print "verify_refund_finalize"
        context.verify_refund_finalize(
            "relayer",
            &pending_keys[0],
            blockhash.clone(),
            1,
            vec![]
        )
    );

    // 7. Cleaned up
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());

    // 8. safe_verify_deposit STILL blocked after verify_refund_finalize
    check!(
        context.safe_verify_deposit("relayer", deposit_msg, tx_bytes, vout, blockhash, 1, vec![]),
        "Already deposit utxo"
    );

    // 9. No nBTC minted
    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 0);
}

// ── refund_address matching tests ──

/// deposit_msg.refund_address is set and matches the provided refund_address — works
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_address_matches_deposit_msg() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "e1e1069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f30",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 50_000)],
    );

    // refund_address matches deposit_msg.refund_address — should succeed
    check!(
        print "request_refund matching address"
        context.request_refund(
            "alice",
            deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![],
            None
        )
    );
}

/// deposit_msg.refund_address is None — provided refund_address is used
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_address_none_in_deposit_msg() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    // No refund_address in deposit_msg
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

    let tx_bytes = generate_transaction_bytes(
        vec![(
            "f2f2069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f31",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 50_000)],
    );

    // refund_address provided externally — should succeed
    check!(
        print "request_refund with external address"
        context.request_refund(
            "alice",
            deposit_msg,
            TARGET_ADDRESS,
            tx_bytes,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![],
            None
        )
    );
}

/// deposit_msg.refund_address is set but doesn't match — should fail
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_address_mismatch() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "a3a3069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f32",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 50_000)],
    );

    let wrong_address = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa";

    // refund_address doesn't match deposit_msg.refund_address — should fail
    check!(
        context.request_refund(
            "alice",
            deposit_msg,
            wrong_address,
            tx_bytes,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
            None
        ),
        "refund_address does not match deposit_msg.refund_address"
    );
}

#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_operator_skips_timelock() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
            "f4f5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f30",
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
            TARGET_ADDRESS,
            tx_bytes.clone(),
            vout,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d"
                .to_string(),
            1,
            vec![],
            None
        )
    );

    let key = utxo_storage_key(&tx_bytes, vout);

    // 2. Set a long timelock so it definitely hasn't passed
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 999999, "unsafe_refund_timelock_sec": 999999}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // 3. Regular user (alice) — blocked by timelock
    check!(
        context.execute_refund("alice", &key),
        "Refund timelock has not passed yet"
    );

    // 4. Grant Operator role to alice
    check!(context.bridge_acl_grant_role(
        "root",
        "Operator",
        &context.get_account_by_name("alice").sdk_id()
    ));

    // 5. Operator (alice) — timelock skipped, execute succeeds
    check!(
        print "execute_refund as operator"
        context.execute_refund("alice", &key)
    );
}

/// Calling `execute_refund` twice on Bitcoin. Unlike Zcash, Bitcoin has no
/// consensus `branch_id`, so the refund transaction is byte-for-byte identical
/// and its txid (= pending id) is the same. The second call therefore tries to
/// insert an already-existing `BTCPendingInfo` and is rejected — there is only
/// ever one refund transaction.
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_execute_twice_same_tx() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let refund_btc_address = TARGET_ADDRESS;
    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(refund_btc_address.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = generate_transaction_bytes(
        vec![(
            "a2a5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f19",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.request_refund(
        "alice",
        deposit_msg.clone(),
        TARGET_ADDRESS,
        tx_bytes.clone(),
        vout,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![],
        None
    ));
    let key = utxo_storage_key(&tx_bytes, vout);

    // Allow two pending refund txs so the second call is not blocked by capacity
    // (we want to observe the same-txid behaviour, not the pending limit).
    let root_id = context.get_account_by_name("root").id().clone();
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "set_pending_tx_limit")
        .args_json(json!({ "account_id": root_id, "max_pending": 2 }))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // root (DAO) fast-tracks the pre-authorized refund address (timelock 0).
    check!(print "execute_refund #1" context.execute_refund("root", &key));
    let pending1 = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(
        pending1.len(),
        1,
        "first execute_refund creates one pending tx"
    );
    let id1 = pending1.keys().next().unwrap().clone();

    // Second call rebuilds the identical refund tx (same id) and is rejected.
    check!(
        context.execute_refund("root", &key),
        "pending info already exist"
    );

    // Still exactly one refund transaction, with the same id as before.
    let pending2 = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(
        pending2.len(),
        1,
        "same txid => the second execute_refund adds no new pending tx"
    );
    assert!(
        pending2.contains_key(&id1),
        "the single pending tx id is unchanged"
    );
}

/// A *different* account cannot duplicate or hijack a refund that is already being
/// executed. Two protections apply on Bitcoin:
///   1. a non-privileged account is still gated by the timelock;
///   2. once past the timelock it rebuilds the identical tx (same id), so the
///      insert is rejected — the pending tx stays owned by the original caller.
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_refund_execute_twice_different_account() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let refund_btc_address = TARGET_ADDRESS;
    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(refund_btc_address.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let tx_bytes = generate_transaction_bytes(
        vec![(
            "a2a5069f02ad4ca31a16113903ab9fe9e8da6ddf20cad4b461b71e8b96050f19",
            0,
            None,
        )],
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;

    check!(context.request_refund(
        "alice",
        deposit_msg.clone(),
        TARGET_ADDRESS,
        tx_bytes.clone(),
        vout,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![],
        None
    ));
    let key = utxo_storage_key(&tx_bytes, vout);

    // Short timelock so we can fast-forward past it deterministically.
    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 200}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    // root (DAO) fast-tracks the pre-authorized address (timelock 0).
    check!(print "execute_refund #1 (root)" context.execute_refund("root", &key));
    let pending1 = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(pending1.len(), 1);
    let id1 = pending1.keys().next().unwrap().clone();

    // (1) A non-privileged different account (bob) is still gated by the timelock.
    check!(
        context.execute_refund("bob", &key),
        "Refund timelock has not passed yet"
    );

    // (2) After the timelock, bob's call reaches the finalize step but rebuilds the
    // identical tx (same id) and is rejected — no duplicate, no hijack.
    worker.fast_forward(4000).await.unwrap();
    check!(
        context.execute_refund("bob", &key),
        "pending info already exist"
    );

    let pending2 = context.get_btc_pending_infos_paged().await.unwrap();
    assert_eq!(
        pending2.len(),
        1,
        "a different account cannot create a second refund tx"
    );
    assert!(
        pending2.contains_key(&id1),
        "the original refund pending tx is unchanged"
    );
}

/// Express the `request_refund` limits in terms of real signed P2PKH inputs: how
/// much storage each costs and how many fit under the `MAX_REQUEST_REFUND_TX_BYTES`
/// (200 KB) cap. Builds transactions with realistic ~148-byte inputs (36-byte outpoint
/// + 1-byte script length + 107-byte scriptSig + 4-byte sequence), measures the
/// on-chain storage each request adds, asserts the 2 NEAR deposit covers the worst
/// case near the cap, and asserts a tx with too many inputs is rejected.
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_request_refund_input_capacity() {
    use bitcoin::{
        absolute::LockTime, consensus::serialize, transaction::Version, Address, Amount, OutPoint,
        ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness,
    };
    use std::str::FromStr;

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: Some(TARGET_ADDRESS.to_string()),
    };
    let deposit_address = context
        .get_user_deposit_address(deposit_msg.clone())
        .await
        .unwrap();
    let deposit_spk = Address::from_str(&deposit_address)
        .unwrap()
        .assume_checked()
        .script_pubkey();

    let required = context.required_balance_for_request_refund().await.unwrap();
    let cost_per_byte = 10u128.pow(19); // 0.00001 NEAR per byte
    println!(
        "==> required_balance_for_request_refund: {:.4} NEAR",
        required.as_yoctonear() as f64 / 1e24
    );

    // Build a tx with `n` realistic signed P2PKH inputs (107-byte scriptSig) paying
    // the deposit address. `seed` keeps each tx (and its UTXO key) distinct.
    let build_tx = |n: usize, seed: u64| -> Vec<u8> {
        let txid = format!("{seed:064x}").parse().unwrap();
        let input: Vec<TxIn> = (0..n)
            .map(|i| TxIn {
                previous_output: OutPoint {
                    txid,
                    vout: i as u32,
                },
                script_sig: ScriptBuf::from_bytes(vec![0u8; 107]),
                sequence: Sequence(0xffff_fffd),
                witness: Witness::new(),
            })
            .collect();
        serialize(&Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input,
            output: vec![TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: deposit_spk.clone(),
            }],
        })
    };

    let mut bytes_per_input = 0.0;
    for n in [1usize, 200, 700, 1_340] {
        let tx_bytes = build_tx(n, n as u64);
        bytes_per_input = tx_bytes.len() as f64 / n as f64;

        let storage_before = context
            .bridge_contract
            .view_account()
            .await
            .unwrap()
            .storage_usage;

        context
            .request_refund(
                "alice",
                deposit_msg.clone(),
                TARGET_ADDRESS,
                tx_bytes.clone(),
                0,
                "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
                1,
                vec![],
                None,
            )
            .await
            .unwrap()
            .unwrap();

        let storage_used = context
            .bridge_contract
            .view_account()
            .await
            .unwrap()
            .storage_usage
            - storage_before;
        let storage_cost_yocto = storage_used as u128 * cost_per_byte;
        println!(
            "==> {n} inputs -> tx_bytes={} ({:.0} B/input), storage={} B ({:.4} NEAR)",
            tx_bytes.len(),
            tx_bytes.len() as f64 / n as f64,
            storage_used,
            storage_cost_yocto as f64 / 1e24,
        );
        // The 2 NEAR deposit must cover real storage for every accepted (<= cap) tx.
        assert!(
            required.as_yoctonear() >= storage_cost_yocto,
            "deposit ({}) does not cover storage ({}) for {n} inputs",
            required.as_yoctonear(),
            storage_cost_yocto,
        );
    }

    // A signed P2PKH input is ~148 bytes, so the 200 KB cap admits ~1350 of them.
    assert!(
        (140.0..=155.0).contains(&bytes_per_input),
        "unexpected per-input size: {bytes_per_input:.1} B"
    );
    let inputs_at_cap = 200_000.0 / bytes_per_input;
    println!("==> inputs that fit under the 200 KB cap: ~{inputs_at_cap:.0}");
    assert!(
        (1_300.0..=1_400.0).contains(&inputs_at_cap),
        "unexpected input capacity: {inputs_at_cap:.0}"
    );

    // A tx with too many inputs (over the 200 KB cap) is rejected outright.
    let big_tx = build_tx(1_400, 999_999);
    assert!(big_tx.len() > 200_000, "test tx should exceed the cap");
    check!(
        context.request_refund(
            "alice",
            deposit_msg.clone(),
            TARGET_ADDRESS,
            big_tx,
            0,
            "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
            1,
            vec![],
            None
        ),
        "tx_bytes too large for refund request"
    );
}

/// After `execute_refund` the UTXO is also inserted into `verified_deposit_utxo` (to
/// block a later deposit) while the request is kept with `executed == true`. That must
/// NOT let a non-privileged account reject the in-flight refund via the "already
/// deposited" path — only DAO/Operator can. Regression test for that access check.
#[tokio::test]
#[cfg(not(feature = "zcash"))]
async fn test_reject_refund_blocked_after_execute() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;

    let deposit_msg = DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
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
        vec![(deposit_address.as_str(), 100_000)],
    );
    let vout: u32 = 0;
    let key = utxo_storage_key(&tx_bytes, vout);

    // Use the success-asserting `check!` form (not `print`, which only logs) so a
    // silent failure here would actually fail the test.
    check!(context.request_refund(
        "alice",
        deposit_msg.clone(),
        TARGET_ADDRESS,
        tx_bytes.clone(),
        vout,
        "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d".to_string(),
        1,
        vec![],
        None
    ));

    context
        .get_account_by_name("root")
        .call(context.bridge_contract.id(), "update_config")
        .args_json(json!({"update": {"refund_timelock_sec": 200}}))
        .deposit(near_sdk::NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();
    worker.fast_forward(4000).await.unwrap();

    // Execute the refund — this inserts the UTXO into verified_deposit_utxo and keeps
    // the request with executed == true. Must actually succeed for the test to be meaningful.
    check!(context.execute_refund("alice", &key));

    // A non-privileged account must NOT be able to reject the now in-flight refund.
    check!(
        context.reject_refund("bob", &key),
        "Only DAO/Operator can reject, or UTXO must be already verified via deposit"
    );

    // DAO can still reject it.
    check!(context.reject_refund("root", &key));
}
