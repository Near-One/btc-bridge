use near_gas::NearGas;
use near_sdk::env::account_balance;
use near_sdk::{serde_json, NearToken};
use near_workspaces::network::Sandbox;
use near_workspaces::Worker;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn gen_bundle_hex(amount: u64) -> String {
    use orchard::builder::{Builder, BundleType};
    use orchard::keys::{FullViewingKey, OutgoingViewingKey, Scope, SpendingKey};
    use orchard::tree::Anchor;
    use orchard::value::NoteValue;
    use rand::rngs::OsRng;
    use zcash_primitives::transaction::components::orchard::write_v5_bundle;

    let mut rng = OsRng;
    let sk = SpendingKey::from_bytes([7u8; 32]).unwrap();
    let fvk = FullViewingKey::from(&sk);
    let recipient = fvk.address_at(0u32, Scope::External);

    let mut builder = Builder::new(BundleType::DEFAULT, Anchor::empty_tree());
    builder
        .add_output(
            Some(OutgoingViewingKey::from([0u8; 32])),
            recipient,
            NoteValue::from_raw(amount),
            [0u8; 512],
        )
        .unwrap();

    let (unauth, _) = builder
        .build::<zcash_protocol::value::ZatBalance>(&mut rng)
        .unwrap()
        .unwrap();
    let pk = orchard::circuit::ProvingKey::build();
    let authorized = unauth
        .create_proof(&pk, &mut rng)
        .unwrap()
        .prepare(&mut rng, [0u8; 32])
        .finalize()
        .unwrap();
    let mut bytes = vec![];
    write_v5_bundle(Some(&authorized), &mut bytes).unwrap();
    hex::encode(bytes)
}

#[tokio::test]
async fn gas_parse_build() {
    println!("Starting worker");
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await.unwrap();

    println!("Compiling contract");
    let wasm = near_workspaces::compile_project(env!("CARGO_MANIFEST_DIR"))
        .await
        .expect("compile orchard verifier");

    println!("Deploying contract");
    let contract = worker.dev_deploy(&wasm).await.unwrap();

    println!("Transferring NEAR to contract");
    let root_account = worker.root_account().unwrap();
    let _result = root_account
        .transfer_near(contract.as_account().id(), NearToken::from_near(1_000_000))
        .await
        .unwrap();

    let contract_account_balance = contract.as_account().view_account().await.unwrap().balance;
    println!(
        "Contract account balance after deploy: {} NEAR",
        contract_account_balance.as_near()
    );

    println!("Initializing contract");
    contract
        .call("new")
        .args_json(serde_json::json!({}))
        .transact()
        .await
        .unwrap()
        .unwrap();

    println!("Generating bundle hex");
    let bundle_hex_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("bundle_hex.txt");
    let bundle_hex = if bundle_hex_path.exists() {
        println!("Loading bundle hex from file");
        std::fs::read_to_string(&bundle_hex_path)
            .unwrap()
            .trim()
            .to_string()
    } else {
        println!("Generating new bundle hex");
        let hex = gen_bundle_hex(50_000);
        std::fs::write(&bundle_hex_path, &hex).unwrap();
        hex
    };

    println!("Calling parse_and_build_only");
    let tx_status = contract
        .call("parse_and_build_only")
        .args_json(serde_json::json!({ "bundle_hex": bundle_hex }))
        .gas(NearGas::from_tgas(300000))
        .transact_async()
        .await
        .unwrap();

    // Manual polling loop with custom logic (e.g., timeout, backoff, logging)
    let mut attempts = 0;
    const MAX_ATTEMPTS: usize = 1000000; // Adjust for timeout (e.g., 100 * 300ms = 30s)
    const POLL_INTERVAL: Duration = Duration::from_secs(30);
    let _result = loop {
        attempts += 1;
        if attempts > MAX_ATTEMPTS {
            panic!("Transaction did not complete within the expected time");
        }

        match tx_status.status().await.unwrap() {
            std::task::Poll::Ready(result) => {
                // Transaction completed
                println!("Transaction finalized: {:#?}", result);
                println!(
                    "parse_and_build_only total_gas_burnt: {} success={} failures={:#?}",
                    result.total_gas_burnt,
                    result.is_success(),
                    result.receipt_failures()
                );
                break;
            }
            std::task::Poll::Pending => {
                // Still pending, wait and retry
                println!("Transaction pending, attempt {}", attempts);
                println!(
                    "Time taken so far: {} seconds",
                    attempts as u64 * POLL_INTERVAL.as_secs()
                );
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
    };

    // Fetch the txhash continuously until the transaction is complete
}
