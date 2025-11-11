use near_sdk::serde_json;
use near_workspaces::network::Sandbox;
use near_workspaces::Worker;

#[tokio::test]
async fn gas_vk_build() {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await.unwrap();
    let wasm = near_workspaces::compile_project(env!("CARGO_MANIFEST_DIR"))
        .await
        .expect("compile orchard verifier");
    let contract = worker.dev_deploy(&wasm).await.unwrap();

    contract
        .call("new")
        .args_json(serde_json::json!({}))
        .transact()
        .await
        .unwrap()
        .unwrap();

    let outcome = contract
        .call("build_vk_only")
        .args_json(serde_json::json!({}))
        .max_gas()
        .transact()
        .await
        .unwrap();

    println!("{:#?}", outcome);
    println!(
        "build_vk_only total_gas_burnt: {} success={} failures={:#?}",
        outcome.total_gas_burnt,
        outcome.is_success(),
        outcome.receipt_failures()
    );
}
