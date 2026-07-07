mod setup;

use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_workspaces::types::NearToken;
use near_workspaces::Contract;
use setup::*;

#[cfg(not(feature = "zcash"))]
const CHAIN: &str = "BitcoinMainnet";
#[cfg(feature = "zcash")]
const CHAIN: &str = "ZcashTestnet";

const HOLDERS_COUNT: usize = 10;

fn holder_ids() -> Vec<String> {
    (0..HOLDERS_COUNT)
        .map(|i| format!("holder{i}.near"))
        .collect()
}

fn holder_amount(index: usize) -> u128 {
    (index as u128 + 1) * 10_000
}

async fn deploy_new_token(context: &Context) -> Contract {
    let account = context
        .root
        .create_subaccount("nbtc2")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await
        .unwrap()
        .unwrap();
    let contract = account
        .deploy(&std::fs::read("../../res/nbtc.wasm").unwrap())
        .await
        .unwrap()
        .unwrap();
    contract
        .call("new")
        .args_json(json!({
            "controller": context.root.id(),
            "bridge_id": context.bridge_contract.id(),
            "name": "Near BTC v2",
            "symbol": "NBTC2",
            "icon": null,
            "decimals": 8,
        }))
        .transact()
        .await
        .unwrap()
        .unwrap();
    contract
}

async fn mint(
    context: &Context,
    token_id: &near_workspaces::AccountId,
    account_id: &str,
    amount: u128,
) {
    context
        .bridge_contract
        .as_account()
        .call(token_id, "mint")
        .args_json(json!({
            "mint_account_id": account_id,
            "mint_amount": U128(amount),
            "protocol_fee": U128(0),
            "relayer_account_id": context.bridge_contract.id(),
            "relayer_fee": U128(0),
            "post_actions": null,
        }))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();
}

async fn balance_of(token: &Contract, account_id: &str) -> u128 {
    token
        .view("ft_balance_of")
        .args_json(json!({ "account_id": account_id }))
        .await
        .unwrap()
        .json::<U128>()
        .unwrap()
        .0
}

async fn total_supply(token: &Contract) -> u128 {
    token
        .view("ft_total_supply")
        .await
        .unwrap()
        .json::<U128>()
        .unwrap()
        .0
}

async fn setup_migration(context: &Context) -> (Contract, Vec<String>) {
    let holders = holder_ids();
    for (index, holder) in holders.iter().enumerate() {
        mint(
            context,
            context.nbtc_contract.id(),
            holder,
            holder_amount(index),
        )
        .await;
    }

    let new_token = deploy_new_token(context).await;

    context
        .root
        .call(context.bridge_contract.id(), "acl_grant_role")
        .args_json(json!({
            "role": "MigrationOperator",
            "account_id": context.relayer.id(),
        }))
        .transact()
        .await
        .unwrap()
        .unwrap();

    (new_token, holders)
}

async fn migrate(
    context: &Context,
    new_token: &near_workspaces::AccountId,
    accounts: &[String],
) -> near_workspaces::result::ExecutionFinalResult {
    context
        .relayer
        .call(context.bridge_contract.id(), "migrate_to_new_token")
        .args_json(json!({
            "new_token": new_token,
            "accounts": accounts,
        }))
        .deposit(NearToken::from_yoctonear(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_migrate_to_new_token_success() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let (new_token, holders) = setup_migration(&context).await;

    let preexisting_holder = "already-on-new-token.near";
    let preexisting_amount = 5_000;
    mint(
        &context,
        new_token.id(),
        preexisting_holder,
        preexisting_amount,
    )
    .await;

    let old_supply = total_supply(&context.nbtc_contract).await;
    let expected_total: u128 = (0..HOLDERS_COUNT).map(holder_amount).sum();
    assert_eq!(old_supply, expected_total);

    let result = migrate(&context, new_token.id(), &holders).await;
    assert!(result.is_success(), "{result:?}");

    for (index, holder) in holders.iter().enumerate() {
        assert_eq!(balance_of(&new_token, holder).await, holder_amount(index));
        assert_eq!(
            balance_of(&context.nbtc_contract, holder).await,
            holder_amount(index),
            "old token balances must remain untouched"
        );
    }
    assert_eq!(
        balance_of(&new_token, preexisting_holder).await,
        preexisting_amount
    );
    assert_eq!(
        total_supply(&new_token).await,
        expected_total + preexisting_amount
    );
    assert_eq!(total_supply(&context.nbtc_contract).await, old_supply);

    let config = context.get_bridge_config().await.unwrap();
    assert_eq!(config.nbtc_account_id.as_str(), new_token.id().as_str());
}

#[tokio::test]
async fn test_migrate_to_new_token_rejects_same_token() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let (_new_token, holders) = setup_migration(&context).await;

    let result = migrate(&context, context.nbtc_contract.id(), &holders).await;
    assert!(result.is_failure());
    assert!(
        format!("{result:?}").contains("New token must differ from the current token"),
        "{result:?}"
    );
}

#[tokio::test]
async fn test_migrate_to_new_token_incomplete_accounts_fails() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let context = Context::new(&worker, Some(CHAIN.to_string())).await;
    let (new_token, holders) = setup_migration(&context).await;

    let incomplete = &holders[..HOLDERS_COUNT - 1];
    let result = migrate(&context, new_token.id(), incomplete).await;
    assert!(result.is_failure());
    assert!(
        format!("{result:?}").contains("Sum of account balances does not match total supply"),
        "{result:?}"
    );

    assert_eq!(total_supply(&new_token).await, 0);
    let config = context.get_bridge_config().await.unwrap();
    assert_eq!(
        config.nbtc_account_id.as_str(),
        context.nbtc_contract.id().as_str()
    );
}
