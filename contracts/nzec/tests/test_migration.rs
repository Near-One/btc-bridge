use near_sdk::serde_json::{json, Value};
use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::types::{AccessKey, Gas, KeyType, NearToken, SecretKey};
use near_workspaces::{Account, Contract, Worker};

/// Old nzec code (layout `ContractV0`: no `bridge_id`). Download the currently
/// deployed wasm and drop it here before running this test.
const OLD_WASM: &str = "tests/data/nzec_v0.wasm";
/// New nzec code built from this branch. Produce it with `make nzec`.
const NEW_WASM: &str = "../../res/nzec.wasm";

const NEW_BRIDGE_ID: &str = "bridge.nzec.test.near";

async fn deploy_and_init_old(worker: &Worker<Sandbox>, owner: &Account) -> Contract {
    let contract = worker
        .dev_deploy(&std::fs::read(OLD_WASM).unwrap())
        .await
        .unwrap();

    // Old `new` signature: (owner_id, metadata) — no `bridge_id` yet.
    contract
        .call("new")
        .args_json(json!({
            "owner_id": owner.id(),
            "metadata": {
                "spec": "ft-1.0.0",
                "name": "nZEC",
                "symbol": "nZEC",
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
async fn test_nzec_migration_via_full_access_key() {
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

    // 2) Deploy the NEW code and call `migrate` in a single batch, signed by the
    //    deployer key. Batched so a failed migration rolls back the deploy.
    deployer
        .batch(contract.id())
        .deploy(&std::fs::read(NEW_WASM).unwrap())
        .call(
            Function::new("migrate")
                .args_json(json!({ "bridge_id": NEW_BRIDGE_ID }))
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

    // The new `bridge_id` getter returns the value passed to `migrate`.
    let bridge_id: String = contract
        .call("bridge_id")
        .view()
        .await
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(bridge_id, NEW_BRIDGE_ID);

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
    assert_eq!(metadata["name"], "nZEC");
    assert_eq!(metadata["symbol"], "nZEC");

    // The deleted key can no longer sign transactions for the contract account.
    let res = deployer
        .batch(contract.id())
        .call(Function::new("bridge_id").gas(Gas::from_tgas(10)))
        .transact()
        .await;
    assert!(
        res.is_err() || res.unwrap().is_failure(),
        "deleted full access key must no longer be usable"
    );
}
