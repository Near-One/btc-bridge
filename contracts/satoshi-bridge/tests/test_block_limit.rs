mod setup;

use near_sdk::serde_json::json;
use near_sdk::NearToken;
use satoshi_bridge::{BlockAmountRing, DepositMsg};
use setup::*;

#[cfg(feature = "zcash")]
const CHAIN: &str = "ZcashTestnet";
#[cfg(not(feature = "zcash"))]
const CHAIN: &str = "BitcoinMainnet";

const BLOCKHASH: &str = "0000000000000c3f818b0b6374c609dd8e548a0a9e61065e942cd466c426e00d";
const NOT_ENOUGH_CONFIRMATIONS_ERR: &str =
    "Not enough confirmations for the rolling-window bridge amount";

// Two-tier strategy: amounts below 10_000 need 3 confirmations,
// amounts from 10_000 up to the max tier need 10.
const LOW_TIER_UPPER_BOUND: u128 = 10_000;
const HIGH_TIER_UPPER_BOUND: u128 = 50_000;
const LOW_TIER_CONFIRMATIONS: u64 = 3;
const HIGH_TIER_CONFIRMATIONS: u64 = 10;

const BASE_BLOCK_HEIGHT: u64 = 100;

// Must match `confirmations_delta` in the test config (`Context::new`).
const RELAYER_CONFIRMATIONS_DELTA: u64 = 1;

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

async fn setup_two_tier_context(
    worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
) -> (Context, String) {
    setup_two_tier_context_with(worker, true).await
}

async fn setup_two_tier_context_with(
    worker: &near_workspaces::Worker<near_workspaces::network::Sandbox>,
    whitelist_relayer: bool,
) -> (Context, String) {
    let context = Context::new(worker, Some(CHAIN.to_string())).await;

    dao_call(
        &context,
        "set_confirmations_strategy",
        json!({
            "range_upper_bound": "10000000",
            "confirmations": HIGH_TIER_CONFIRMATIONS,
        }),
    )
    .await;
    dao_call(
        &context,
        "set_confirmations_strategy",
        json!({
            "range_upper_bound": LOW_TIER_UPPER_BOUND.to_string(),
            "confirmations": LOW_TIER_CONFIRMATIONS,
        }),
    )
    .await;
    dao_call(
        &context,
        "set_confirmations_strategy",
        json!({
            "range_upper_bound": HIGH_TIER_UPPER_BOUND.to_string(),
            "confirmations": HIGH_TIER_CONFIRMATIONS,
        }),
    )
    .await;
    dao_call(
        &context,
        "remove_confirmations_strategy",
        json!({ "range_upper_bound": "10000000" }),
    )
    .await;

    dao_call(
        &context,
        "update_config",
        json!({ "update": { "min_deposit_amount": "1000" } }),
    )
    .await;

    if whitelist_relayer {
        dao_call(
            &context,
            "extend_relayer_white_list",
            json!({ "relayer_ids": [context.relayer.id()] }),
        )
        .await;
    }

    let deposit_address = context
        .get_user_deposit_address(alice_deposit_msg(&context))
        .await
        .unwrap();

    (context, deposit_address)
}

fn alice_deposit_msg(context: &Context) -> DepositMsg {
    DepositMsg {
        recipient_id: context.alice.sdk_id(),
        post_actions: None,
        extra_msg: None,
        safe_deposit: None,
        refund_address: None,
    }
}

async fn set_heights(context: &Context, tx_block_height: u64, tip_height: u64) {
    for (method, height) in [
        ("set_tx_block_height", tx_block_height),
        ("set_last_block_height", tip_height),
    ] {
        context
            .root
            .call(context.btc_light_client_contract.id(), method)
            .args_json(json!({ "height": height }))
            .max_gas()
            .transact()
            .await
            .unwrap()
            .unwrap();
    }
}

async fn verify_deposit(
    context: &Context,
    deposit_address: &str,
    amount: u64,
    input_tx_salt: u64,
) -> near_workspaces::Result<near_workspaces::result::ExecutionFinalResult> {
    let input_tx_id = format!("{input_tx_salt:064x}");
    let tx_bytes = generate_transaction_bytes(
        vec![(input_tx_id.as_str(), 0, None)],
        vec![(deposit_address, amount)],
    );
    context
        .verify_deposit_v2(
            "relayer",
            alice_deposit_msg(context),
            tx_bytes,
            0,
            proof_json(BLOCKHASH.to_string(), 1, vec![]),
        )
        .await
}

#[tokio::test]
async fn test_low_tier_amount_passes_with_low_tier_confirmations() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    set_heights(&context, BASE_BLOCK_HEIGHT, BASE_BLOCK_HEIGHT + 1).await;
    check!(
        verify_deposit(&context, &deposit_address, 5000, 1),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 5000, 1));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 5000);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 1);
}

#[tokio::test]
async fn test_mid_amount_requires_high_tier_confirmations() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, 25_000, 1),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 2,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, 25_000, 1),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 25_000, 1));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 25_000);
}

#[tokio::test]
async fn test_same_block_cumulative_amount_escalates_tier() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 1));

    check!(
        verify_deposit(&context, &deposit_address, 6000, 2),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 2));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 12_000);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 2);
}

#[tokio::test]
async fn test_consecutive_blocks_cannot_reuse_the_low_tier() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    // 6000 on its own is a low-tier amount, so 3 confirmations are enough.
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 1));

    // Repeating it in the next block would leave 12_000 exposed to a 4-block
    // reorg, which only the high tier covers.
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT + 1,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, 6000, 2),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    // It passes once the window holding both blocks is high-tier deep, i.e. when
    // the older of the two blocks is `HIGH_TIER_CONFIRMATIONS` deep.
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT + 1,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 2));

    // Same story one block further along.
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT + 2,
        BASE_BLOCK_HEIGHT + 2 + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, 6000, 3),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    // Block 100 has aged out of the widest window by now, so only blocks 101 and
    // 102 share it: 12_000, which the high-tier depth accepts.
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT + 2,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 3));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 18_000);
}

#[tokio::test]
async fn test_block_deeper_than_the_widest_window_is_unconstrained() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    let config = context.get_bridge_config().await.unwrap();
    let capacity = u64::try_from(BlockAmountRing::capacity_for(&config)).unwrap();
    assert!(
        capacity + LOW_TIER_CONFIRMATIONS > HIGH_TIER_CONFIRMATIONS,
        "the original height must end up deeper than the widest window"
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 1));

    check!(
        verify_deposit(&context, &deposit_address, 6000, 2),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 2));

    let wrapped_height = BASE_BLOCK_HEIGHT + capacity;
    set_heights(
        &context,
        wrapped_height,
        wrapped_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 3));

    // Back at the original height, now a full ring behind the tip: no window
    // reaches it, so the tier table cannot ask for more confirmations.
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        wrapped_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 4));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 24_000);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 4);
}

#[tokio::test]
async fn test_non_whitelisted_relayer_requires_confirmations_delta() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context_with(&worker, false).await;

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, 5000, 1),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS + RELAYER_CONFIRMATIONS_DELTA - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 5000, 1));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 5000);
}

#[tokio::test]
async fn test_non_whitelisted_relayer_delta_applies_to_high_tier() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context_with(&worker, false).await;

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, 25_000, 1),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS + RELAYER_CONFIRMATIONS_DELTA - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 25_000, 1));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 25_000);
}

#[tokio::test]
async fn test_tier_boundary_amount_falls_into_higher_tier() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    let amount = u64::try_from(LOW_TIER_UPPER_BOUND).unwrap();
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, amount, 1),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, amount, 1));

    assert_eq!(
        context.ft_balance_of("alice").await.unwrap().0,
        LOW_TIER_UPPER_BOUND
    );
}

#[tokio::test]
async fn test_ring_grow_preserves_block_cumulative() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 1));
    check!(
        verify_deposit(&context, &deposit_address, 6000, 2),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    dao_call(
        &context,
        "update_config",
        json!({ "update": { "extra_msg_confirmations_delta": 6 } }),
    )
    .await;

    check!(
        verify_deposit(&context, &deposit_address, 6000, 2),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + HIGH_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 2));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 12_000);
}

#[tokio::test]
async fn test_tier_boundary_update_applies_to_existing_cumulative() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 1));
    check!(
        verify_deposit(&context, &deposit_address, 6000, 2),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    dao_call(
        &context,
        "set_confirmations_strategy",
        json!({
            "range_upper_bound": "20000",
            "confirmations": LOW_TIER_CONFIRMATIONS,
        }),
    )
    .await;

    check!(verify_deposit(&context, &deposit_address, 6000, 2));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 12_000);
    assert_eq!(context.get_utxos_paged().await.unwrap().len(), 2);
}

#[tokio::test]
async fn test_ring_shrink_merges_collided_blocks() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    let config = context.get_bridge_config().await.unwrap();
    let capacity_before = u64::try_from(BlockAmountRing::capacity_for(&config)).unwrap();
    // Dropping both deltas (1 + 1 in the test config) shrinks the ring by 2.
    let capacity_after = capacity_before - 2;
    let old_height = BASE_BLOCK_HEIGHT;
    let new_height = BASE_BLOCK_HEIGHT + capacity_after;

    set_heights(
        &context,
        old_height,
        old_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 1));
    check!(
        verify_deposit(&context, &deposit_address, 6000, 2),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    set_heights(
        &context,
        new_height,
        new_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 3));
    check!(
        verify_deposit(&context, &deposit_address, 6000, 4),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    dao_call(
        &context,
        "update_config",
        json!({ "update": { "confirmations_delta": 0, "extra_msg_confirmations_delta": 0 } }),
    )
    .await;

    set_heights(
        &context,
        old_height,
        new_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 2));

    set_heights(
        &context,
        new_height,
        new_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(
        verify_deposit(&context, &deposit_address, 6000, 4),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );
    check!(verify_deposit(&context, &deposit_address, 3000, 5));

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 21_000);
}

#[tokio::test]
async fn test_ring_grow_undercounts_an_evicted_block() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let (context, deposit_address) = setup_two_tier_context(&worker).await;

    let config = context.get_bridge_config().await.unwrap();
    let capacity = u64::try_from(BlockAmountRing::capacity_for(&config)).unwrap();
    let evicting_height = BASE_BLOCK_HEIGHT + capacity;
    // The raised top tier must exceed the evicted block's final depth, otherwise
    // that depth alone satisfies it and the under-count is not observable.
    let raised_tier = capacity + 13;

    // A middle tier that separates the amount the ring remembers (12_000) from
    // the amount it should have remembered (18_000).
    dao_call(
        &context,
        "set_confirmations_strategy",
        json!({
            "range_upper_bound": "15000",
            "confirmations": LOW_TIER_CONFIRMATIONS,
        }),
    )
    .await;

    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        BASE_BLOCK_HEIGHT + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 1));

    set_heights(
        &context,
        evicting_height,
        evicting_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 2));

    dao_call(
        &context,
        "set_confirmations_strategy",
        json!({
            "range_upper_bound": HIGH_TIER_UPPER_BOUND.to_string(),
            "confirmations": raised_tier,
        }),
    )
    .await;

    // The raised tier makes the window wide enough to cover both heights again,
    // but the ring no longer remembers the first 6000: it sees 12_000 (low tier,
    // accepted) where the true exposure is 18_000 (top tier, which this depth
    // would not satisfy).
    set_heights(
        &context,
        BASE_BLOCK_HEIGHT,
        evicting_height + LOW_TIER_CONFIRMATIONS - 1,
    )
    .await;
    check!(verify_deposit(&context, &deposit_address, 6000, 3));
    // One more and even the under-count reaches the top tier.
    check!(
        verify_deposit(&context, &deposit_address, 6000, 4),
        NOT_ENOUGH_CONFIRMATIONS_ERR
    );

    assert_eq!(context.ft_balance_of("alice").await.unwrap().0, 18_000);
}
