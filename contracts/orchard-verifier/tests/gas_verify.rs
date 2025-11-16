use near_sdk::serde_json;
use near_workspaces::network::Sandbox;
use near_workspaces::Worker;
use std::fs;
use std::path::Path;

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
async fn gas_verify() {
    println!("Starting worker");
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await.unwrap();
    println!("Compiling contract");
    let wasm = near_workspaces::compile_project(env!("CARGO_MANIFEST_DIR"))
        .await
        .expect("compile orchard verifier");
    println!("Deploying contract");
    let contract = worker.dev_deploy(&wasm).await.unwrap();

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

    println!("Calling verify_orchard_bundle");
    let outcome = contract
        .call("verify_orchard_bundle")
        .args_json(serde_json::json!({ "bundle_hex": bundle_hex }))
        .max_gas()
        .transact()
        .await
        .unwrap();

    println!("{:#?}", outcome);
    println!(
        "verify_orchard_bundle total_gas_burnt: {} success={} failures={:#?}",
        outcome.total_gas_burnt,
        outcome.is_success(),
        outcome.receipt_failures()
    );
}
