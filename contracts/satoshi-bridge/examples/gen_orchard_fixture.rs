// cargo run -p satoshi-bridge --example gen_orchard_fixture --features zcash -- \
//   --network test --amount 50000
// Prints a Unified Address and Orchard bundle hex recoverable with OVK=00..00.

#[cfg(feature = "zcash")]
fn main() {
    use orchard::builder::{Builder, BundleType};
    use orchard::keys::{FullViewingKey, OutgoingViewingKey, Scope, SpendingKey};
    use orchard::tree::Anchor;
    use orchard::value::NoteValue;
    use rand::rngs::OsRng;
    use std::env;
    use zcash_address::unified::{Encoding, Receiver};
    use zcash_address::ToAddress;
    use zcash_primitives::transaction::components::orchard::write_v5_bundle;

    let args: Vec<String> = env::args().collect();
    let mut amount: u64 = 50_000;
    let mut net = "test".to_string();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--amount" => {
                amount = args[i + 1].parse().expect("amount u64");
                i += 2;
            }
            "--network" => {
                net = args[i + 1].clone();
                i += 2;
            }
            _ => i += 1,
        }
    }

    let mut rng = OsRng;
    let sk = SpendingKey::from_bytes([7u8; 32]).unwrap();
    let fvk = FullViewingKey::from(&sk);
    let recipient = fvk.address_at(0u32, Scope::External);

    // Build a simple output-only bundle with OVK = 00..00
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

    // Produce Unified Address string containing the Orchard receiver.
    let orchard_raw = recipient.to_raw_address_bytes();
    let ua = zcash_address::unified::Address::try_from_items(vec![Receiver::Orchard(orchard_raw)])
        .expect("UA from orchard receiver");
    let network = match net.as_str() {
        "main" | "mainnet" => zcash_protocol::consensus::NetworkType::Main,
        _ => zcash_protocol::consensus::NetworkType::Test,
    };
    let ua_str = zcash_address::ZcashAddress::from_unified(network, ua).encode();

    // Serialize bundle to v5 bytes
    let mut bytes = vec![];
    write_v5_bundle(Some(&authorized), &mut bytes).unwrap();

    println!("UA={}", ua_str);
    println!("BUNDLE_HEX={}", hex::encode(bytes));
}

#[cfg(not(feature = "zcash"))]
fn main() {
    eprintln!("Enable --features zcash to build this example");
}
