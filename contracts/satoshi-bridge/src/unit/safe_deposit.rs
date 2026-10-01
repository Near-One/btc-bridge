//! Failed safe deposit (the recipient's `ft_on_transfer` refunds everything) seen through the
//! supply monitor: S = nBTC total supply, U = available UTXOs, I = `DepositInProgress` UTXOs.
//! With no pending withdrawals the monitor requires `K <= S - U <= K + I` after every receipt.
//! Here K = 0 and S is tracked by hand, since `safe_mint` and `burn` run on the nBTC contract.

use crate::*;
use near_sdk::mock::MockAction;
use near_sdk::test_utils::get_created_receipts;
use near_sdk::PromiseResult;

const DEPOSIT_AMOUNT: u64 = 100_000;

fn bridge_id() -> AccountId {
    "bridge_id".parse().unwrap()
}

fn relayer_id() -> AccountId {
    "relayer_id".parse().unwrap()
}

fn pending_utxo_info() -> PendingUTXOInfo {
    let tx_id = "1f".repeat(32);
    PendingUTXOInfo {
        utxo_storage_key: generate_utxo_storage_key(tx_id.clone(), 0),
        tx_id,
        utxo: UTXO {
            path: "path".to_string(),
            tx_bytes: Vec::new(),
            vout: 0,
            balance: DEPOSIT_AMOUNT,
        },
    }
}

/// Set up the environment of a bridge callback receipt that sees `promise_result`.
fn set_callback_env(unit_env: &mut UnitEnv, promise_result: PromiseResult) {
    testing_env!(
        unit_env
            .context
            .current_account_id(bridge_id())
            .predecessor_account_id(bridge_id())
            .signer_account_id(relayer_id())
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![promise_result],
    );
}

fn monitor_utxo_sums(contract: &Contract) -> (u128, u128) {
    let available: u128 = contract
        .get_utxos_paged(None, None)
        .values()
        .map(|utxo| u128::from(utxo.balance))
        .sum();
    let in_progress: u128 = contract
        .get_utxos_in_progress_paged(None, None)
        .into_values()
        .map(|status| match status {
            UTXOStatus::DepositInProgress(vutxo) => u128::from(UTXO::from(vutxo).balance),
        })
        .sum();
    (available, in_progress)
}

fn assert_monitor_band(contract: &Contract, total_supply: u128, step: &str) {
    let (available, in_progress) = monitor_utxo_sums(contract);
    let residual = i128::try_from(total_supply).unwrap() - i128::try_from(available).unwrap();
    assert!(
        residual >= 0 && residual <= i128::try_from(in_progress).unwrap(),
        "{step}: S - U = {residual} is outside [0, I = {in_progress}]"
    );
}

fn is_in_progress(contract: &Contract, utxo_storage_key: &str) -> bool {
    contract
        .get_utxos_in_progress_paged(None, None)
        .contains_key(utxo_storage_key)
}

fn is_verified(contract: &Contract, utxo_storage_key: &str) -> bool {
    contract
        .data()
        .verified_deposit_utxo
        .contains(utxo_storage_key)
}

fn function_call_receipt_index(receiver_id: &AccountId, method: &str) -> Option<usize> {
    get_created_receipts().iter().position(|receipt| {
        &receipt.receiver_id == receiver_id
            && receipt.actions.iter().any(|action| {
                matches!(action, MockAction::FunctionCallWeight { method_name, .. }
                    if method_name == method.as_bytes())
            })
    })
}

/// Run the flow up to `safe_mint_callback` and return the total supply at that point.
fn run_until_burn_dispatched(unit_env: &mut UnitEnv) -> u128 {
    let utxo_storage_key = pending_utxo_info().utxo_storage_key;
    let mut total_supply = 0;
    assert_monitor_band(&unit_env.contract, total_supply, "before deposit");

    // Light client confirmed the deposit: the UTXO goes in progress before safe_mint.
    set_callback_env(unit_env, PromiseResult::Successful(b"true".to_vec()));
    let _ = unit_env.contract.verify_safe_deposit_callback(
        recipient_id(),
        U128(DEPOSIT_AMOUNT.into()),
        String::new(),
        pending_utxo_info(),
    );
    assert!(is_in_progress(&unit_env.contract, &utxo_storage_key));
    assert_monitor_band(
        &unit_env.contract,
        total_supply,
        "after verify_safe_deposit_callback",
    );

    // safe_mint raises S; ft_on_transfer fails and the tokens go back to the bridge.
    total_supply += u128::from(DEPOSIT_AMOUNT);
    assert_monitor_band(&unit_env.contract, total_supply, "after safe_mint");

    // safe_mint_callback sees the full refund and dispatches the burn.
    set_callback_env(unit_env, PromiseResult::Successful(b"\"0\"".to_vec()));
    assert!(!unit_env.contract.safe_mint_callback(
        recipient_id(),
        U128(DEPOSIT_AMOUNT.into()),
        pending_utxo_info(),
    ));
    let burn_index = function_call_receipt_index(&nbtc_id(), "burn").expect("burn not dispatched");
    let callback_index = function_call_receipt_index(&bridge_id(), "safe_deposit_burn_callback")
        .expect("burn callback not dispatched");
    assert_eq!(
        get_created_receipts()[callback_index].receipt_indices,
        vec![u64::try_from(burn_index).unwrap()],
        "burn callback must wait for the burn"
    );
    assert!(is_in_progress(&unit_env.contract, &utxo_storage_key));
    assert!(is_verified(&unit_env.contract, &utxo_storage_key));
    assert_monitor_band(&unit_env.contract, total_supply, "after safe_mint_callback");

    total_supply
}

/// Run the whole failed safe deposit flow with the given burn outcome.
fn run_failed_safe_deposit(burn_succeeds: bool) -> UnitEnv {
    let mut unit_env = init_unit_env();
    let utxo_storage_key = pending_utxo_info().utxo_storage_key;
    let mut total_supply = run_until_burn_dispatched(&mut unit_env);

    // burn lowers S only if it succeeds. The UTXO is still in progress either way.
    if burn_succeeds {
        total_supply -= u128::from(DEPOSIT_AMOUNT);
    }
    assert_monitor_band(&unit_env.contract, total_supply, "after burn");

    let burn_result = if burn_succeeds {
        PromiseResult::Successful(Vec::new())
    } else {
        PromiseResult::Failed
    };
    set_callback_env(&mut unit_env, burn_result);
    assert_eq!(
        unit_env
            .contract
            .safe_deposit_burn_callback(utxo_storage_key),
        burn_succeeds
    );
    assert_monitor_band(
        &unit_env.contract,
        total_supply,
        "after safe_deposit_burn_callback",
    );
    assert!(unit_env.contract.get_utxos_paged(None, None).is_empty());

    unit_env
}

#[test]
fn test_failed_safe_deposit_releases_utxo_after_burn() {
    let unit_env = run_failed_safe_deposit(true);
    let utxo_storage_key = pending_utxo_info().utxo_storage_key;
    assert!(!is_in_progress(&unit_env.contract, &utxo_storage_key));
    assert!(!is_verified(&unit_env.contract, &utxo_storage_key));
}

#[test]
fn test_failed_safe_deposit_keeps_utxo_in_progress_if_burn_fails() {
    let unit_env = run_failed_safe_deposit(false);
    let utxo_storage_key = pending_utxo_info().utxo_storage_key;
    assert!(is_in_progress(&unit_env.contract, &utxo_storage_key));
    assert!(is_verified(&unit_env.contract, &utxo_storage_key));
}

#[test]
fn test_failed_safe_deposit_can_be_retried_after_burn() {
    let mut unit_env = run_failed_safe_deposit(true);
    set_callback_env(&mut unit_env, PromiseResult::Successful(b"true".to_vec()));
    let _ = unit_env.contract.verify_safe_deposit_callback(
        recipient_id(),
        U128(DEPOSIT_AMOUNT.into()),
        String::new(),
        pending_utxo_info(),
    );
    assert!(is_in_progress(
        &unit_env.contract,
        &pending_utxo_info().utxo_storage_key
    ));
}

#[test]
#[should_panic(expected = "Already deposit utxo")]
fn test_failed_safe_deposit_retry_is_rejected_while_burn_is_in_flight() {
    let mut unit_env = init_unit_env();
    run_until_burn_dispatched(&mut unit_env);
    set_callback_env(&mut unit_env, PromiseResult::Successful(b"true".to_vec()));
    let _ = unit_env.contract.verify_safe_deposit_callback(
        recipient_id(),
        U128(DEPOSIT_AMOUNT.into()),
        String::new(),
        pending_utxo_info(),
    );
}

#[test]
#[should_panic(expected = "Already deposit utxo")]
fn test_failed_safe_deposit_standard_retry_is_rejected_while_burn_is_in_flight() {
    let mut unit_env = init_unit_env();
    run_until_burn_dispatched(&mut unit_env);
    set_callback_env(&mut unit_env, PromiseResult::Successful(b"true".to_vec()));
    let _ = unit_env.contract.verify_deposit_callback(
        recipient_id(),
        U128(DEPOSIT_AMOUNT.into()),
        U128(0),
        U128(0),
        pending_utxo_info(),
        None,
    );
}

#[test]
#[should_panic(expected = "Already deposit utxo")]
fn test_failed_safe_deposit_retry_is_rejected_after_burn_failure() {
    let mut unit_env = run_failed_safe_deposit(false);
    set_callback_env(&mut unit_env, PromiseResult::Successful(b"true".to_vec()));
    let _ = unit_env.contract.verify_safe_deposit_callback(
        recipient_id(),
        U128(DEPOSIT_AMOUNT.into()),
        String::new(),
        pending_utxo_info(),
    );
}
