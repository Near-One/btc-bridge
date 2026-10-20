use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{PromiseResult, RuntimeFeesConfig};

const AMOUNT: u64 = 100_000;
const TX_ID: &str = "83d28cfcff0d86035e2742ecde99ef4801bb9f928d7d0118a0c1dd87bdc299ac";
const KEY: &str = "83d28cfcff0d86035e2742ecde99ef4801bb9f928d7d0118a0c1dd87bdc299ac@0";
const OTHER_KEY: &str = "83d28cfcff0d86035e2742ecde99ef4801bb9f928d7d0118a0c1dd87bdc299ac@1";

fn pending_utxo_info_for(vout: usize) -> PendingUTXOInfo {
    PendingUTXOInfo {
        tx_id: TX_ID.to_string(),
        utxo_storage_key: format!("{TX_ID}@{vout}"),
        utxo: UTXO {
            path: "path".to_string(),
            tx_bytes: Vec::new(),
            vout,
            balance: AMOUNT,
        },
    }
}

fn pending_utxo_info() -> PendingUTXOInfo {
    pending_utxo_info_for(0)
}

fn add_pending_deposit(contract: &mut Contract, info: &PendingUTXOInfo) {
    contract
        .data_mut()
        .verified_deposit_utxo
        .insert(info.utxo_storage_key.clone());
    contract.internal_set_utxo_in_progress(
        &info.utxo_storage_key,
        UTXOStatus::DepositInProgress(info.utxo.clone().into()),
    );
}

fn env_with_pending_deposit() -> UnitEnv {
    let mut unit_env = init_unit_env();
    add_pending_deposit(&mut unit_env.contract, &pending_utxo_info());
    unit_env
}

fn run_safe_mint_callback_for(
    unit_env: &mut UnitEnv,
    info: PendingUTXOInfo,
    result: PromiseResult,
) -> bool {
    testing_env!(
        unit_env.context.build(),
        near_sdk::test_vm_config(),
        RuntimeFeesConfig::test(),
        Default::default(),
        vec![result],
    );
    unit_env
        .contract
        .safe_mint_callback(recipient_id(), U128(AMOUNT.into()), info)
}

fn run_safe_mint_callback(unit_env: &mut UnitEnv, result: PromiseResult) -> bool {
    run_safe_mint_callback_for(unit_env, pending_utxo_info(), result)
}

fn env_with_two_unresolved_deposits() -> UnitEnv {
    let mut unit_env = env_with_pending_deposit();
    add_pending_deposit(&mut unit_env.contract, &pending_utxo_info_for(1));
    run_safe_mint_callback_for(&mut unit_env, pending_utxo_info(), PromiseResult::Failed);
    run_safe_mint_callback_for(
        &mut unit_env,
        pending_utxo_info_for(1),
        PromiseResult::Failed,
    );
    unit_env
}

fn success_result(value: &[u8]) -> PromiseResult {
    PromiseResult::Successful(value.to_vec())
}

fn dao_context(context: &mut VMContextBuilder) {
    testing_env!(context
        .predecessor_account_id(owner_id())
        .attached_deposit(NearToken::from_yoctonear(1))
        .build());
}

fn is_verified(contract: &Contract) -> bool {
    contract.data().verified_deposit_utxo.contains(KEY)
}

fn is_unresolved(contract: &Contract) -> bool {
    matches!(
        contract.data().utxos_in_progress.get(KEY),
        Some(UTXOStatus::DepositMintUnresolved(_))
    )
}

fn has_utxo(contract: &Contract) -> bool {
    contract.data().utxos.contains_key(KEY)
}

#[test]
fn failed_result_leaves_deposit_unresolved() {
    let mut unit_env = env_with_pending_deposit();
    assert!(!run_safe_mint_callback(
        &mut unit_env,
        PromiseResult::Failed
    ));
    assert!(is_verified(&unit_env.contract));
    assert!(is_unresolved(&unit_env.contract));
    assert!(!has_utxo(&unit_env.contract));
}

#[test]
fn malformed_result_leaves_deposit_unresolved() {
    let mut unit_env = env_with_pending_deposit();
    assert!(!run_safe_mint_callback(
        &mut unit_env,
        success_result(b"not-u128")
    ));
    assert!(is_verified(&unit_env.contract));
    assert!(is_unresolved(&unit_env.contract));
    assert!(!has_utxo(&unit_env.contract));
}

#[test]
fn zero_result_releases_deposit() {
    let mut unit_env = env_with_pending_deposit();
    assert!(!run_safe_mint_callback(
        &mut unit_env,
        success_result(b"\"0\"")
    ));
    assert!(!is_verified(&unit_env.contract));
    assert!(unit_env
        .contract
        .data()
        .utxos_in_progress
        .get(KEY)
        .is_none());
    assert!(!has_utxo(&unit_env.contract));
}

#[test]
fn non_zero_result_registers_utxo() {
    let mut unit_env = env_with_pending_deposit();
    assert!(run_safe_mint_callback(
        &mut unit_env,
        success_result(b"\"1\"")
    ));
    assert!(is_verified(&unit_env.contract));
    assert!(unit_env
        .contract
        .data()
        .utxos_in_progress
        .get(KEY)
        .is_none());
    assert!(has_utxo(&unit_env.contract));
}

#[test]
fn confirm_unresolved_deposit_mint_registers_utxo() {
    let mut unit_env = env_with_pending_deposit();
    run_safe_mint_callback(&mut unit_env, PromiseResult::Failed);
    dao_context(&mut unit_env.context);
    unit_env
        .contract
        .confirm_unresolved_deposit_mints(vec![KEY.to_string()]);
    assert!(is_verified(&unit_env.contract));
    assert!(unit_env
        .contract
        .data()
        .utxos_in_progress
        .get(KEY)
        .is_none());
    assert!(has_utxo(&unit_env.contract));
}

#[test]
fn reject_unresolved_deposit_mint_releases_deposit() {
    let mut unit_env = env_with_pending_deposit();
    run_safe_mint_callback(&mut unit_env, PromiseResult::Failed);
    dao_context(&mut unit_env.context);
    unit_env
        .contract
        .reject_unresolved_deposit_mints(vec![KEY.to_string()]);
    assert!(!is_verified(&unit_env.contract));
    assert!(unit_env
        .contract
        .data()
        .utxos_in_progress
        .get(KEY)
        .is_none());
    assert!(!has_utxo(&unit_env.contract));
}

#[test]
#[should_panic(expected = "Deposit mint is not unresolved")]
fn confirm_rejects_deposit_still_in_progress() {
    let mut unit_env = env_with_pending_deposit();
    dao_context(&mut unit_env.context);
    unit_env
        .contract
        .confirm_unresolved_deposit_mints(vec![KEY.to_string()]);
}

#[test]
#[should_panic(expected = "Deposit mint is not unresolved")]
fn reject_rejects_deposit_still_in_progress() {
    let mut unit_env = env_with_pending_deposit();
    dao_context(&mut unit_env.context);
    unit_env
        .contract
        .reject_unresolved_deposit_mints(vec![KEY.to_string()]);
}

#[test]
#[should_panic(expected = "Insufficient permissions")]
fn resolve_requires_dao() {
    let mut unit_env = env_with_pending_deposit();
    run_safe_mint_callback(&mut unit_env, PromiseResult::Failed);
    testing_env!(unit_env
        .context
        .predecessor_account_id(user_id())
        .attached_deposit(NearToken::from_yoctonear(1))
        .build());
    unit_env
        .contract
        .reject_unresolved_deposit_mints(vec![KEY.to_string()]);
}

#[test]
fn confirm_resolves_batch() {
    let mut unit_env = env_with_two_unresolved_deposits();
    dao_context(&mut unit_env.context);
    unit_env
        .contract
        .confirm_unresolved_deposit_mints(vec![KEY.to_string(), OTHER_KEY.to_string()]);
    let data = unit_env.contract.data();
    assert!(data.utxos.contains_key(KEY) && data.utxos.contains_key(OTHER_KEY));
    assert!(data.utxos_in_progress.is_empty());
}

#[test]
fn reject_resolves_batch() {
    let mut unit_env = env_with_two_unresolved_deposits();
    dao_context(&mut unit_env.context);
    unit_env
        .contract
        .reject_unresolved_deposit_mints(vec![KEY.to_string(), OTHER_KEY.to_string()]);
    let data = unit_env.contract.data();
    assert!(!data.verified_deposit_utxo.contains(KEY));
    assert!(!data.verified_deposit_utxo.contains(OTHER_KEY));
    assert!(data.utxos_in_progress.is_empty());
}

#[test]
#[should_panic(expected = "Deposit mint is not unresolved")]
fn batch_fails_entirely_if_any_key_is_not_unresolved() {
    let mut unit_env = env_with_pending_deposit();
    run_safe_mint_callback(&mut unit_env, PromiseResult::Failed);
    add_pending_deposit(&mut unit_env.contract, &pending_utxo_info_for(1));
    dao_context(&mut unit_env.context);
    unit_env
        .contract
        .confirm_unresolved_deposit_mints(vec![KEY.to_string(), OTHER_KEY.to_string()]);
}

#[test]
fn deposit_mint_resolver_role_can_resolve() {
    let mut unit_env = env_with_pending_deposit();
    run_safe_mint_callback(&mut unit_env, PromiseResult::Failed);
    testing_env!(unit_env.context.predecessor_account_id(owner_id()).build());
    unit_env
        .contract
        .acl_grant_role(Role::DepositMintResolver.into(), user_id());
    testing_env!(unit_env
        .context
        .predecessor_account_id(user_id())
        .attached_deposit(NearToken::from_yoctonear(1))
        .build());
    unit_env
        .contract
        .reject_unresolved_deposit_mints(vec![KEY.to_string()]);
    assert!(!is_verified(&unit_env.contract));
}
