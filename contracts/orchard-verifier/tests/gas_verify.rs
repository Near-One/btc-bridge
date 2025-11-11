use near_sdk::serde_json;
use near_workspaces::network::Sandbox;
use near_workspaces::Worker;

fn gen_bundle_hex(amount: u64) -> String {
    use orchard::builder::{BundleType, Builder};
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

    let bundle_hex = gen_bundle_hex(50_000);

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
