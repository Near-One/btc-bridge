use near_sdk::serde_json::{json, Value};
use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::types::{AccessKey, Gas, KeyType, NearToken, SecretKey};
use near_workspaces::{Account, Contract, Worker};
use std::collections::HashSet;

/// Old PoA token wasm (state layout: `{ token, metadata }` + `__OWNER__` key).
const OLD_WASM: &str = "tests/data/nzec_v0.wasm";
/// New nbtc code built from this branch. Produce it with `make nbtc`.
const NEW_WASM: &str = "../../res/nbtc.wasm";

const NEW_WITHDRAW_RELAYER: &str = "relayer.nbtc.test.near";

async fn deploy_and_init_old(worker: &Worker<Sandbox>, owner: &Account) -> Contract {
    let contract = worker
        .dev_deploy(&std::fs::read(OLD_WASM).unwrap())
        .await
        .unwrap();

    // Old `new` signature: (owner_id, metadata).
    contract
        .call("new")
        .args_json(json!({
            "owner_id": owner.id(),
            "metadata": {
                "spec": "ft-1.0.0",
                "name": "nBTC",
                "symbol": "nBTC",
                "decimals": 8,
            }
        }))
        .transact()
        .await
        .unwrap()
        .unwrap();

    contract
}

#[tokio::test]
async fn test_migration_from_poa_via_full_access_key() {
    let worker = near_workspaces::sandbox().await.unwrap();
    let owner = worker.dev_create_account().await.unwrap();
    let user = worker.dev_create_account().await.unwrap();

    // Deploy + init the OLD contract, then seed state to prove it survives.
    let contract = deploy_and_init_old(&worker, &owner).await;

    // owner-only `ft_deposit` credits `user`; attach NEAR to cover its storage.
    owner
        .call(contract.id(), "ft_deposit")
        .args_json(json!({
            "owner_id": user.id(),
            "amount": "1000",
            "memo": null,
        }))
        .deposit(NearToken::from_near(1))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();

    let balance_before: String = contract
        .call("ft_balance_of")
        .args_json(json!({ "account_id": user.id() }))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(balance_before, "1000");

    // Snapshot the full storage layout of the OLD contract. Sanity-check the
    // assumptions the migration relies on: metadata lives at key 0x01
    // (Prefix::Metadata) and the Ownable key is present.
    let state_before = worker.view_state(contract.id()).await.unwrap();
    assert!(
        state_before.contains_key(&vec![1u8]),
        "old contract must store metadata at key 0x01"
    );
    assert!(state_before.contains_key(&b"__OWNER__".to_vec()));

    let metadata_before: Value = contract
        .call("ft_metadata")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();

    // ----- Migration flow -----

    // 1) Add a full access key held by the "deployer" to the contract account.
    let deployer_sk = SecretKey::from_random(KeyType::ED25519);
    let deployer_pk = deployer_sk.public_key();
    contract
        .as_account()
        .batch(contract.id())
        .add_key(deployer_pk.clone(), AccessKey::full_access())
        .transact()
        .await
        .unwrap()
        .unwrap();

    // The deployer signs as the contract account using the freshly added key.
    let deployer = Account::from_secret_key(contract.id().clone(), deployer_sk, &worker);

    // 2) Deploy the NEW code and call `migrate_from_poa` in a single batch,
    //    signed by the deployer key. Batched so a failed migration rolls back
    //    the deploy.
    deployer
        .batch(contract.id())
        .deploy(&std::fs::read(NEW_WASM).unwrap())
        .call(
            Function::new("migrate_from_poa")
                .args_json(json!({
                    "controller": owner.id(),
                    "bridge_id": owner.id(),
                    "withdraw_relayer": NEW_WITHDRAW_RELAYER,
                }))
                .gas(Gas::from_tgas(200)),
        )
        .transact()
        .await
        .unwrap()
        .unwrap();

    // 3) Delete the full access key (re-lock the account).
    deployer
        .batch(contract.id())
        .delete_key(deployer_pk.clone())
        .transact()
        .await
        .unwrap()
        .unwrap();

    // ----- Assertions -----

    // Token state survived the migration.
    let balance_after: String = contract
        .call("ft_balance_of")
        .args_json(json!({ "account_id": user.id() }))
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(balance_after, "1000", "balance must survive migration");

    let metadata: Value = contract
        .call("ft_metadata")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(metadata["name"], "nBTC");
    assert_eq!(metadata["symbol"], "nBTC");
    assert_eq!(metadata["decimals"], 8);
    assert_eq!(
        metadata, metadata_before,
        "ft_metadata must be identical to the pre-migration metadata"
    );

    // Storage stays clean: the new metadata overwrites key 0x01 in place, so
    // the only expected diff is `__OWNER__` removed and the withdraw relayer
    // key added. Any orphaned leftovers would show up as an extra key here.
    let state_after = worker.view_state(contract.id()).await.unwrap();
    let mut expected_keys: HashSet<Vec<u8>> = state_before.keys().cloned().collect();
    expected_keys.remove(&b"__OWNER__".to_vec());
    expected_keys.insert(b"WITHDRAW_RELAYER_ADDRESS".to_vec());
    let actual_keys: HashSet<Vec<u8>> = state_after.keys().cloned().collect();
    assert_eq!(
        actual_keys, expected_keys,
        "migration must not leave orphaned storage keys"
    );
    assert_eq!(
        state_after[&vec![1u8]],
        state_before[&vec![1u8]],
        "metadata at key 0x01 must be byte-identical after migration"
    );

    // New code is running.
    let version: String = contract
        .call("version")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(version, env!("CARGO_PKG_VERSION"));

    // ----- From-scratch deployment must produce an identical state -----

    // Deploy the same code fresh and drive it to the same logical state:
    // same controller/bridge (owner), same holder with the same balance,
    // same withdraw relayer.
    let fresh = worker
        .dev_deploy(&std::fs::read(NEW_WASM).unwrap())
        .await
        .unwrap();
    fresh
        .call("new")
        .args_json(json!({
            "controller": owner.id(),
            "bridge_id": owner.id(),
            "name": "nBTC",
            "symbol": "nBTC",
            "icon": null,
            "decimals": 8,
        }))
        .transact()
        .await
        .unwrap()
        .unwrap();
    owner
        .call(fresh.id(), "mint")
        .args_json(json!({
            "mint_account_id": user.id(),
            "mint_amount": "1000",
            "protocol_fee": "0",
            "relayer_account_id": owner.id(),
            "relayer_fee": "0",
            "post_actions": null,
        }))
        .max_gas()
        .transact()
        .await
        .unwrap()
        .unwrap();
    owner
        .call(fresh.id(), "set_withdraw_relayer_address")
        .args_json(json!({ "relayer": NEW_WITHDRAW_RELAYER }))
        .transact()
        .await
        .unwrap()
        .unwrap();

    let fresh_metadata: Value = fresh
        .call("ft_metadata")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(fresh_metadata, metadata);

    // A fresh `new()` registers the bridge account, while the old PoA
    // contract never registered its owner. Mirror the real post-migration
    // runbook step: register the bridge on the migrated token, otherwise the
    // first `ft_transfer_call` to it would fail.
    owner
        .call(contract.id(), "storage_deposit")
        .args_json(json!({ "account_id": owner.id(), "registration_only": true }))
        .deposit(NearToken::from_millinear(10))
        .transact()
        .await
        .unwrap()
        .unwrap();

    let migrated_state = worker.view_state(contract.id()).await.unwrap();
    let fresh_state = worker.view_state(fresh.id()).await.unwrap();
    assert_eq!(
        migrated_state, fresh_state,
        "migrated state must be byte-identical to a from-scratch deployment"
    );

    let migrated_usage = worker
        .view_account(contract.id())
        .await
        .unwrap()
        .storage_usage;
    let fresh_usage = worker.view_account(fresh.id()).await.unwrap().storage_usage;
    assert_eq!(
        migrated_usage, fresh_usage,
        "storage usage must match a from-scratch deployment"
    );

    // The deleted key can no longer sign transactions for the contract account.
    let res = deployer
        .batch(contract.id())
        .call(Function::new("version").gas(Gas::from_tgas(10)))
        .transact()
        .await;
    assert!(
        res.is_err() || res.unwrap().is_failure(),
        "deleted full access key must no longer be usable"
    );
}
