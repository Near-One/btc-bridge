mod setup;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use bitcoin::{Amount, OutPoint, TxOut};
use near_sdk::serde_json::json;
use near_sdk::NearToken;
use near_workspaces::result::ExecutionFinalResult;
use satoshi_bridge::network::{Address, Chain};
use satoshi_bridge::{BTCPendingInfo, DepositMsg, TokenReceiverMessage};
use setup::*;

#[cfg(feature = "zcash")]
const CHAIN: &str = "ZcashTestnet";
#[cfg(not(feature = "zcash"))]
const CHAIN: &str = "BitcoinMainnet";

#[cfg(feature = "zcash")]
const TARGET_ADDRESS: &str = "tmD67UTsZ4iBbhCae4D43k1x8fhFNhwd4Jn";
#[cfg(not(feature = "zcash"))]
const TARGET_ADDRESS: &str = "1PAGsaT5vDz6hjzvuenSw33hWzESTR3ZHQ";

#[cfg(not(feature = "zcash"))]
use satoshi_bridge::bitcoin_utils::types::ChainSpecificData;

const BLOCKHASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";
const DEPOSIT_AMOUNT: u64 = 100_000;
const TGAS: u64 = 1_000_000_000_000;

/// Prepaid gas the relayer attaches to `sign_btc_transaction_batch` for
/// `n` inputs. Tests use exactly this value so the formula stays valid.
fn batch_prepaid_tgas(n: usize) -> u64 {
    60 + 30 * n as u64
}

fn get_chain() -> Chain {
    if cfg!(feature = "zcash") {
        Chain::ZcashTestnet
    } else {
        Chain::BitcoinMainnet
    }
}

/// Covers the ZIP-317 minimum fee (5000 zats per logical action) on Zcash.
fn btc_gas_fee(input_num: usize) -> u64 {
    std::cmp::max(25_000, 5_000 * input_num as u64)
}

async fn dao_call(context: &Context, method: &str, args: near_sdk::serde_json::Value) {
    context
        .root
        .call(context.bridge_contract.id(), method)
        .args_json(args)
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();
}

async fn setup_context(
    worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
) -> Context {
    let context = Context::new(worker, Some(CHAIN.to_string())).await;
    dao_call(
        &context,
        "update_config",
        json!({ "update": {
            "max_withdrawal_input_number": 30,
            "max_btc_gas_fee": "500000",
        } }),
    )
    .await;
    check!(context.storage_deposit("nbtc", "bridge"));
    if cfg!(feature = "zcash") {
        check!(context.storage_deposit("nbtc", "alice"));
    }
    context
}

/// `extra_msg` changes the deposit path, so each deposit gets its own key,
/// as on mainnet, where withdrawal inputs come from different deposit
/// addresses. That makes the callback derive one public key per input.
fn alice_deposit_msg(context: &Context, extra_msg: String) -> DepositMsg {
    DepositMsg {
        recipient_id: context.get_account_by_name("alice").sdk_id(),
        post_actions: None,
        extra_msg: Some(extra_msg),
        safe_deposit: if cfg!(feature = "zcash") {
            Some(satoshi_bridge::SafeDepositMsg { msg: String::new() })
        } else {
            None
        },
        refund_address: None,
    }
}

/// Deposit address of each derivation path used by `deposit_utxos`. Paths
/// and addresses depend only on the deposit message, so they are the same in
/// every test of this binary.
static DEPOSIT_ADDRESSES: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// Deposit `n` UTXOs of `DEPOSIT_AMOUNT` each to alice and return them.
/// `salt` keeps the deposit transactions distinct across calls.
async fn deposit_utxos(context: &Context, n: usize, salt: usize) -> Vec<OutPoint> {
    let before: HashSet<String> = context
        .get_utxos_paged()
        .await
        .unwrap()
        .into_keys()
        .collect();
    for i in 0..n {
        let deposit_msg = alice_deposit_msg(context, format!("{salt}-{i}"));
        let deposit_address = context
            .get_user_deposit_address(deposit_msg.clone())
            .await
            .unwrap();
        DEPOSIT_ADDRESSES.lock().unwrap().insert(
            satoshi_bridge::get_deposit_path(&deposit_msg),
            deposit_address.clone(),
        );
        let prev_txid = format!("{:064x}", salt * 1000 + i + 1);
        check!(context.verify_deposit_v2(
            "relayer",
            deposit_msg,
            generate_transaction_bytes(
                vec![(prev_txid.as_str(), 0, None)],
                vec![(deposit_address.as_str(), DEPOSIT_AMOUNT)],
            ),
            0,
            proof_json(BLOCKHASH.to_string(), 1, vec![]),
        ));
    }
    let out_points: Vec<OutPoint> = context
        .get_utxos_paged()
        .await
        .unwrap()
        .into_keys()
        .filter(|k| !before.contains(k))
        .map(|k| {
            let (txid, vout) = k.split_once('@').unwrap();
            OutPoint {
                txid: txid.parse().unwrap(),
                vout: vout.parse().unwrap(),
            }
        })
        .collect();
    assert_eq!(out_points.len(), n);
    out_points
}

/// Withdraw `amount` for alice with the given transaction and return the
/// new pending sign id.
async fn withdraw_with(
    context: &Context,
    amount: u128,
    target_btc_address: &str,
    input: Vec<OutPoint>,
    output: Vec<TxOut>,
    chain_specific_data: Option<ChainSpecificData>,
) -> String {
    let before: HashSet<String> = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .into_keys()
        .collect();
    check!(context.do_withdraw(
        "alice",
        "bridge",
        amount,
        TokenReceiverMessage::Withdraw {
            target_btc_address: target_btc_address.to_string(),
            input,
            output,
            max_gas_fee: None,
            chain_specific_data,
        }
    ));
    let mut new_ids: Vec<String> = context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .into_keys()
        .filter(|k| !before.contains(k))
        .collect();
    assert_eq!(new_ids.len(), 1);
    new_ids.pop().unwrap()
}

fn script_pubkey(addr: &str) -> bitcoin::ScriptBuf {
    Address::parse(addr, get_chain())
        .unwrap()
        .script_pubkey()
        .unwrap()
}

/// Withdraw all of `inputs` for alice to a transparent address and return
/// the pending sign id.
async fn withdraw(context: &Context, inputs: Vec<OutPoint>) -> String {
    let config = context.get_bridge_config().await.unwrap();
    let change_address = context.get_change_address().await.unwrap();
    let total_in = DEPOSIT_AMOUNT * inputs.len() as u64;
    let gas_fee = btc_gas_fee(inputs.len());
    let withdraw_amount = u128::from(total_in);
    let withdraw_fee = config.withdraw_bridge_fee.get_fee(withdraw_amount) as u64;
    withdraw_with(
        context,
        withdraw_amount,
        TARGET_ADDRESS,
        inputs,
        vec![
            TxOut {
                value: Amount::from_sat(total_in - gas_fee - withdraw_fee),
                script_pubkey: script_pubkey(TARGET_ADDRESS),
            },
            TxOut {
                value: Amount::from_sat(withdraw_fee),
                script_pubkey: script_pubkey(change_address.as_str()),
            },
        ],
        None,
    )
    .await
}

async fn new_withdrawal(context: &Context, n: usize, salt: usize) -> String {
    let inputs = deposit_utxos(context, n, salt).await;
    withdraw(context, inputs).await
}

async fn pending_info(context: &Context, id: &str) -> BTCPendingInfo {
    context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .remove(id)
        .unwrap()
}

fn signed_indexes(info: &BTCPendingInfo) -> Vec<usize> {
    info.signatures
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.as_ref().map(|_| i))
        .collect()
}

/// `s` of the mock MPC signature for this payload and path (see the mock's
/// `sign_helper`).
fn mock_signature_s(payload: &[u8; 32], path: &str) -> String {
    use bitcoin::hashes::{sha256, Hash};
    hex::encode_upper(
        sha256::Hash::hash(&[payload.as_slice(), path.as_bytes()].concat()).to_byte_array(),
    )
}

/// Signature (DER plus sighash type) and public key of every input of a
/// signed transaction.
#[cfg(not(feature = "zcash"))]
fn signed_tx_input_signatures(tx_bytes: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let tx: bitcoin::Transaction = bitcoin::consensus::deserialize(tx_bytes).unwrap();
    tx.input
        .iter()
        .map(|input| {
            assert_eq!(input.witness.len(), 2);
            (
                input.witness.nth(0).unwrap().to_vec(),
                input.witness.nth(1).unwrap().to_vec(),
            )
        })
        .collect()
}

/// Signature (DER plus sighash type) and public key of every input of a
/// signed transaction.
#[cfg(feature = "zcash")]
fn signed_tx_input_signatures(tx_bytes: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let tx = satoshi_bridge::WrappedTransaction::decode(tx_bytes, &get_chain()).unwrap();
    tx.inner_tx
        .transparent_bundle()
        .unwrap()
        .vin
        .iter()
        .map(|input| {
            let pushes: Vec<Vec<u8>> = bitcoin::Script::from_bytes(&input.script_sig().0 .0)
                .instructions()
                .map(|instruction| match instruction.unwrap() {
                    bitcoin::script::Instruction::PushBytes(bytes) => bytes.as_bytes().to_vec(),
                    op => panic!("unexpected opcode in scriptSig: {op:?}"),
                })
                .collect();
            assert_eq!(pushes.len(), 2);
            (pushes[0].clone(), pushes[1].clone())
        })
        .collect()
}

/// Checks that every stored signature belongs to its own input. The mock MPC
/// returns `s = sha256(payload || path)`, so signature `i` must match the
/// sighash and derivation path of input `i`. For a fully signed transaction
/// it also checks that input `i` of the final transaction carries signature
/// `i` and a public key whose address is the input's deposit address.
async fn assert_signatures_match_inputs(context: &Context, id: &str) {
    let info = pending_info(context, id).await;
    let psbt = info.get_psbt();
    for (i, signature) in info.signatures.iter().enumerate() {
        if let Some(signature) = signature {
            let expected = mock_signature_s(&psbt.get_hash_to_sign(i), &info.vutxos[i].get_path());
            assert_eq!(
                signature.s.scalar, expected,
                "signature {i} is not for input {i}"
            );
        }
    }

    let Some(tx_bytes) = &info.tx_bytes_with_sign else {
        return;
    };
    let inputs = signed_tx_input_signatures(tx_bytes);
    assert_eq!(inputs.len(), info.signatures.len());
    let addresses = DEPOSIT_ADDRESSES.lock().unwrap();
    for (i, (signature, public_key)) in inputs.into_iter().enumerate() {
        let signature = bitcoin::ecdsa::Signature::from_slice(&signature).unwrap();
        assert_eq!(
            hex::encode_upper(&signature.signature.serialize_compact()[32..]),
            info.signatures[i].as_ref().unwrap().s.scalar,
            "input {i} of the signed transaction carries another signature"
        );
        let address = Address::from_pubkey(
            get_chain(),
            bitcoin::PublicKey::from_slice(&public_key).unwrap(),
        )
        .unwrap()
        .to_string();
        assert_eq!(
            address,
            addresses[&info.vutxos[i].get_path()],
            "input {i} of the signed transaction has another input's public key"
        );
    }
}

async fn sign_batch_with_deposit(
    context: &Context,
    caller: &str,
    id: &str,
    indexes: Vec<usize>,
    deposit: NearToken,
) -> ExecutionFinalResult {
    let indexes_len = std::cmp::max(indexes.len(), 1);
    context
        .get_account_by_name(caller)
        .call(context.bridge_contract.id(), "sign_btc_transaction_batch")
        .args_json(json!({
            "btc_pending_sign_id": id,
            "sign_indexes": indexes,
            "key_version": 0,
        }))
        .gas(near_workspaces::types::Gas::from_tgas(batch_prepaid_tgas(
            indexes_len,
        )))
        .deposit(deposit)
        .transact()
        .await
        .unwrap()
}

/// Attaches one yoctoNEAR per requested index, as the relayer does.
async fn sign_batch(context: &Context, id: &str, indexes: Vec<usize>) -> ExecutionFinalResult {
    let deposit = NearToken::from_yoctonear(indexes.len() as u128);
    sign_batch_with_deposit(context, "relayer", id, indexes, deposit).await
}

fn failure_text(outcome: &ExecutionFinalResult) -> String {
    outcome
        .receipt_failures()
        .iter()
        .map(|f| format!("{:?}", f))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_ok(outcome: &ExecutionFinalResult) {
    assert!(
        outcome.is_success() && outcome.receipt_failures().is_empty(),
        "{}",
        failure_text(outcome)
    );
}

fn assert_err(outcome: &ExecutionFinalResult, msg: &str) {
    let text = failure_text(outcome);
    assert!(text.contains(msg), "expected `{msg}`, got: {text}");
}

fn count_logs(outcome: &ExecutionFinalResult, event: &str) -> usize {
    outcome
        .logs()
        .iter()
        .filter(|l| l.contains(&format!("\"event\":\"{event}\"")))
        .count()
}

// ---------------------------------------------------------------------------
// Gas profile
// ---------------------------------------------------------------------------

#[derive(Default, Debug)]
struct GasReport {
    /// Gas burnt by the receipt that runs the public sign method, one entry per call.
    sign_call_tgas: Vec<f64>,
    /// Gas burnt by each `sign_btc_transaction_callback` receipt.
    callback_tgas: Vec<f64>,
    /// Gas burnt by the whole transaction, one entry per call.
    tx_total_tgas: Vec<f64>,
}

fn tgas(gas: near_workspaces::types::Gas) -> f64 {
    gas.as_gas() as f64 / TGAS as f64
}

fn collect_gas(report: &mut GasReport, outcome: &ExecutionFinalResult, bridge: &str) {
    assert_ok(outcome);
    let receipts = outcome.receipt_outcomes();
    report.sign_call_tgas.push(tgas(receipts[0].gas_burnt));
    for r in receipts.iter().skip(1) {
        if r.executor_id.as_str() == bridge
            && r.logs.iter().any(|l| {
                l.contains("\"event\":\"btc_input_signature\"")
                    || l.contains("\"event\":\"signed_btc_transaction\"")
            })
        {
            report.callback_tgas.push(tgas(r.gas_burnt));
        }
    }
    report.tx_total_tgas.push(tgas(outcome.total_gas_burnt));
}

fn sum(v: &[f64]) -> f64 {
    v.iter().sum()
}

fn print_report(label: &str, n: usize, r: &GasReport) {
    println!(
        "GAS {CHAIN} {label} N={n}: sign_calls={} sum_sign_call={:.2} Tgas [first={:.2} last={:.2}] \
         callbacks={} sum_callback={:.2} Tgas [min={:.2} max={:.2}] tx_total_sum={:.2} Tgas",
        r.sign_call_tgas.len(),
        sum(&r.sign_call_tgas),
        r.sign_call_tgas.first().copied().unwrap_or_default(),
        r.sign_call_tgas.last().copied().unwrap_or_default(),
        r.callback_tgas.len(),
        sum(&r.callback_tgas),
        r.callback_tgas.iter().copied().fold(f64::INFINITY, f64::min),
        r.callback_tgas.iter().copied().fold(0.0, f64::max),
        sum(&r.tx_total_tgas),
    );
}

fn assert_fully_signed(infos: &HashMap<String, BTCPendingInfo>, id: &str) {
    let info = infos.get(id).unwrap();
    assert!(info.signatures.iter().all(Option::is_some));
    assert!(info.tx_bytes_with_sign.is_some());
    info.assert_pending_verify();
    println!(
        "signed tx: {} inputs, {} bytes",
        info.signatures.len(),
        info.tx_bytes_with_sign.as_ref().unwrap().len()
    );
}

/// Gas profile of `sign_btc_transaction` (one call per input). Prints one
/// `GAS <chain> single` line per input count.
#[tokio::test]
async fn test_gas_sign_single() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let bridge = context.bridge_contract.id().to_string();
    for (salt, n) in [1usize, 6, 14, 23].into_iter().enumerate() {
        let id = new_withdrawal(&context, n, salt + 1).await;
        let mut report = GasReport::default();
        for i in 0..n {
            let outcome = context
                .sign_btc_transaction("relayer", &id, i, 0)
                .await
                .unwrap();
            collect_gas(&mut report, &outcome, &bridge);
        }
        print_report("single", n, &report);
        assert_fully_signed(&context.get_btc_pending_infos_paged().await.unwrap(), &id);
        assert_signatures_match_inputs(&context, &id).await;
    }
}

/// Gas profile of `sign_btc_transaction_batch` signing every input in one
/// call. Prints one `GAS <chain> batch` line per input count.
#[tokio::test]
async fn test_gas_sign_batch() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let bridge = context.bridge_contract.id().to_string();
    for (salt, n) in [1usize, 6, 14, 20, 30].into_iter().enumerate() {
        let id = new_withdrawal(&context, n, salt + 1).await;
        let outcome = sign_batch(&context, &id, (0..n).collect()).await;
        let mut report = GasReport::default();
        collect_gas(&mut report, &outcome, &bridge);
        print_report("batch", n, &report);
        // One joint callback stores all signatures. It completes the
        // transaction, so it emits only `signed_btc_transaction`.
        assert_eq!(report.callback_tgas.len(), 1);
        assert_eq!(count_logs(&outcome, "btc_input_signature"), 0);
        assert_eq!(count_logs(&outcome, "signed_btc_transaction"), 1);
        assert_fully_signed(&context.get_btc_pending_infos_paged().await.unwrap(), &id);
        assert_signatures_match_inputs(&context, &id).await;
        // Protocol limit on total log length per receipt: 16384 bytes.
        let log_len: usize = outcome
            .logs()
            .iter()
            .filter(|l| l.contains("\"event\":\"signed_btc_transaction\""))
            .map(|l| l.len())
            .sum();
        println!("signed_btc_transaction log: {log_len} bytes");
    }
}

/// Deposits the mock MPC contract received, in `sign` call order.
fn mpc_deposits(outcome: &ExecutionFinalResult) -> Vec<u128> {
    outcome
        .logs()
        .iter()
        .filter_map(|l| l.strip_prefix("sign deposit "))
        .map(|d| d.parse().unwrap())
        .collect()
}

// ---------------------------------------------------------------------------
// Behaviour
// ---------------------------------------------------------------------------

/// Two partial batches sign the whole transaction; the second one also
/// skips an index that the first one already signed. The signed transaction
/// then verifies as usual.
#[tokio::test]
async fn test_batch_sign_partial_then_complete() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let id = new_withdrawal(&context, 4, 1).await;

    let outcome = sign_batch(&context, &id, vec![2, 0]).await;
    assert_ok(&outcome);
    assert_eq!(count_logs(&outcome, "btc_input_signature"), 2);
    assert_eq!(count_logs(&outcome, "signed_btc_transaction"), 0);
    let info = pending_info(&context, &id).await;
    assert_eq!(signed_indexes(&info), vec![0, 2]);
    info.assert_pending_sign();
    assert!(info.tx_bytes_with_sign.is_none());
    assert_signatures_match_inputs(&context, &id).await;
    let alice = context.get_account("alice").await.unwrap().unwrap();
    assert!(alice.btc_pending_sign_ids.contains(&id));

    // Index 0 is already signed, so only 1 and 3 are requested: 2 yoctoNEAR is enough.
    let outcome = sign_batch_with_deposit(
        &context,
        "relayer",
        &id,
        vec![3, 0, 1],
        NearToken::from_yoctonear(2),
    )
    .await;
    assert_ok(&outcome);
    assert_eq!(count_logs(&outcome, "btc_input_signature"), 0);
    assert_eq!(count_logs(&outcome, "signed_btc_transaction"), 1);
    assert!(outcome.json::<bool>().unwrap());
    let infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_fully_signed(&infos, &id);
    assert_signatures_match_inputs(&context, &id).await;
    let alice = context.get_account("alice").await.unwrap().unwrap();
    assert!(alice.btc_pending_sign_ids.is_empty());
    assert!(alice.btc_pending_verify_list.contains(&id));

    check!(context.verify_withdraw_v2(
        "relayer",
        &id,
        proof_json(BLOCKHASH.to_string(), 1, vec![])
    ));
    assert!(context
        .get_btc_pending_infos_paged()
        .await
        .unwrap()
        .is_empty());
}

/// Batch and single signing can be mixed on one transaction, and both paths
/// store each signature at its own input.
#[tokio::test]
async fn test_batch_sign_mixed_with_single() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;

    let single_id = new_withdrawal(&context, 3, 1).await;
    for i in 0..3 {
        check!(context.sign_btc_transaction("relayer", &single_id, i, 0));
    }

    let mixed_id = new_withdrawal(&context, 3, 2).await;
    check!(context.sign_btc_transaction("relayer", &mixed_id, 1, 0));
    // Duplicates are ignored and index 1 is skipped: 2 inputs to sign.
    let outcome = sign_batch_with_deposit(
        &context,
        "relayer",
        &mixed_id,
        vec![0, 1, 2, 2, 0],
        NearToken::from_yoctonear(2),
    )
    .await;
    assert_ok(&outcome);
    let infos = context.get_btc_pending_infos_paged().await.unwrap();
    assert_fully_signed(&infos, &single_id);
    assert_fully_signed(&infos, &mixed_id);
    assert_signatures_match_inputs(&context, &single_id).await;
    assert_signatures_match_inputs(&context, &mixed_id).await;
}

#[tokio::test]
async fn test_batch_sign_rejects_bad_requests() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let id = new_withdrawal(&context, 3, 1).await;

    // One yoctoNEAR short.
    let outcome = sign_batch_with_deposit(
        &context,
        "relayer",
        &id,
        vec![0, 1, 2],
        NearToken::from_yoctonear(2),
    )
    .await;
    assert_err(&outcome, "Attach at least 1 yoctoNEAR per input to sign");

    let outcome = sign_batch_with_deposit(
        &context,
        "relayer",
        &id,
        vec![0],
        NearToken::from_yoctonear(0),
    )
    .await;
    assert_err(&outcome, "Attach at least 1 yoctoNEAR per input to sign");

    let outcome = sign_batch(&context, &id, vec![0, 3]).await;
    assert_err(&outcome, "Invalid sign index");

    let outcome = sign_batch(&context, &id, vec![]).await;
    assert_err(&outcome, "Already signed");

    check!(context.sign_btc_transaction("relayer", &id, 0, 0));
    let outcome = sign_batch(&context, &id, vec![0]).await;
    assert_err(&outcome, "Already signed");

    // A caller without the trusted relayer role.
    let unauthorized = worker.dev_create_account().await.unwrap();
    let outcome = unauthorized
        .call(context.bridge_contract.id(), "sign_btc_transaction_batch")
        .args_json(json!({
            "btc_pending_sign_id": id,
            "sign_indexes": [1, 2],
            "key_version": 0,
        }))
        .gas(near_workspaces::types::Gas::from_tgas(300))
        .deposit(NearToken::from_yoctonear(2))
        .transact()
        .await
        .unwrap();
    assert_err(&outcome, "Relayer is not active");

    // Nothing above changed the signatures.
    assert_eq!(signed_indexes(&pending_info(&context, &id).await), vec![0]);
}

/// Indexes in any order: promise result `i` belongs to the `i`-th index
/// after sorting, not to input `i`.
#[tokio::test]
async fn test_batch_sign_non_sequential_indexes() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let id = new_withdrawal(&context, 4, 1).await;

    assert_ok(&sign_batch(&context, &id, vec![3, 0, 2]).await);
    let info = pending_info(&context, &id).await;
    assert_eq!(signed_indexes(&info), vec![0, 2, 3]);
    assert_signatures_match_inputs(&context, &id).await;

    assert_ok(&sign_batch(&context, &id, vec![1]).await);
    assert_fully_signed(&context.get_btc_pending_infos_paged().await.unwrap(), &id);
    assert_signatures_match_inputs(&context, &id).await;
}

/// A failed MPC request does not affect the other inputs of the batch: their
/// signatures are stored, and the failed input can be requested again.
#[tokio::test]
async fn test_batch_sign_skips_failed_mpc_result() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let id = new_withdrawal(&context, 4, 1).await;

    let failing_path = pending_info(&context, &id).await.vutxos[2].get_path();
    check!(context
        .chain_signatures_contract
        .call("set_fail_paths")
        .args_json(json!({ "paths": [failing_path] }))
        .transact());

    let outcome = sign_batch(&context, &id, (0..4).collect()).await;
    // The `sign` receipt for input 2 fails; the joint callback still runs.
    assert_err(&outcome, "Sign request failed");
    assert_eq!(count_logs(&outcome, "btc_input_signature"), 3);
    assert_eq!(count_logs(&outcome, "signed_btc_transaction"), 0);
    assert!(!outcome.json::<bool>().unwrap());
    let info = pending_info(&context, &id).await;
    assert_eq!(signed_indexes(&info), vec![0, 1, 3]);
    info.assert_pending_sign();
    assert_signatures_match_inputs(&context, &id).await;

    check!(context
        .chain_signatures_contract
        .call("set_fail_paths")
        .args_json(json!({ "paths": Vec::<String>::new() }))
        .transact());
    let outcome = sign_batch(&context, &id, (0..4).collect()).await;
    assert_ok(&outcome);
    // Only input 2 was still unsigned, so it got the whole 4 yoctoNEAR.
    assert_eq!(mpc_deposits(&outcome), vec![4]);
    assert!(outcome.json::<bool>().unwrap());
    assert_fully_signed(&context.get_btc_pending_infos_paged().await.unwrap(), &id);
    assert_signatures_match_inputs(&context, &id).await;
}

/// The whole attached deposit goes to the MPC `sign` calls; the first call
/// also gets the remainder of the even split.
#[tokio::test]
async fn test_batch_sign_forwards_whole_deposit() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let id = new_withdrawal(&context, 3, 1).await;

    let outcome = sign_batch_with_deposit(
        &context,
        "relayer",
        &id,
        vec![0, 1, 2],
        NearToken::from_yoctonear(8),
    )
    .await;
    assert_ok(&outcome);
    let mut deposits = mpc_deposits(&outcome);
    deposits.sort_unstable();
    assert_eq!(deposits, vec![2, 2, 4]);
    assert_fully_signed(&context.get_btc_pending_infos_paged().await.unwrap(), &id);
    assert_signatures_match_inputs(&context, &id).await;
}

/// The batch method shares the `sign_btc_transaction` pause switch.
#[tokio::test]
async fn test_batch_sign_paused_with_single() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    let id = new_withdrawal(&context, 2, 1).await;

    check!(context.bridge_pa_pause_feature("root", "sign_btc_transaction"));
    let outcome = sign_batch(&context, &id, vec![0, 1]).await;
    assert_err(&outcome, "Pausable: Method is paused");

    check!(context.bridge_pa_unpause_feature("root", "sign_btc_transaction"));
    assert_ok(&sign_batch(&context, &id, vec![0, 1]).await);
    assert_fully_signed(&context.get_btc_pending_infos_paged().await.unwrap(), &id);
    assert_signatures_match_inputs(&context, &id).await;
}

/// Batch signing of Zcash withdrawals to an Orchard address: the PSBT carries
/// an Orchard bundle, which the callback parses and writes back. Prints one
/// `GAS <chain> batch-orchard` line per input count.
#[cfg(feature = "zcash")]
#[tokio::test]
async fn test_gas_sign_batch_orchard() {
    // Matches the cached fixture `tests/orchard_bundle_cache_170000.txt`.
    const ORCHARD_AMOUNT: u64 = 170_000;
    const WITHDRAW_FEE: u64 = 20_000;

    let worker = near_workspaces::sandbox().await.unwrap();
    let context = setup_context(&worker).await;
    check!(context.set_withdraw_bridge_fee(WITHDRAW_FEE as u128, 0, 9000));
    let (recipient_ua, bundle_hex) = get_or_gen_bundle(ORCHARD_AMOUNT);
    let change_address = context.get_change_address().await.unwrap();
    let bridge = context.bridge_contract.id().to_string();

    for (salt, n) in [3usize, 6, 14, 30].into_iter().enumerate() {
        let inputs = deposit_utxos(&context, n, salt + 1).await;
        // ZIP-317: transparent inputs plus Orchard actions.
        let gas_fee = 5_000 * (n as u64 + 2);
        let id = withdraw_with(
            &context,
            u128::from(ORCHARD_AMOUNT + gas_fee + WITHDRAW_FEE),
            &recipient_ua,
            inputs,
            vec![TxOut {
                value: Amount::from_sat(DEPOSIT_AMOUNT * n as u64 - ORCHARD_AMOUNT - gas_fee),
                script_pubkey: script_pubkey(change_address.as_str()),
            }],
            Some(ChainSpecificData {
                orchard_bundle_bytes: hex::decode(&bundle_hex).unwrap().into(),
                expiry_height: 10000,
            }),
        )
        .await;
        let outcome = sign_batch(&context, &id, (0..n).collect()).await;
        let mut report = GasReport::default();
        collect_gas(&mut report, &outcome, &bridge);
        print_report("batch-orchard", n, &report);
        assert_eq!(count_logs(&outcome, "signed_btc_transaction"), 1);
        assert_fully_signed(&context.get_btc_pending_infos_paged().await.unwrap(), &id);
        assert_signatures_match_inputs(&context, &id).await;
    }
}
