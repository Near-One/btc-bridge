#![cfg(feature = "zcash")]

use orchard::builder::{Builder, BundleType};
use orchard::keys::{FullViewingKey, OutgoingViewingKey, Scope, SpendingKey};
use orchard::tree::Anchor;
use orchard::value::NoteValue;
use rand::rngs::OsRng;
use zcash_address::unified::{Encoding, Receiver};
use zcash_address::ToAddress;
use zcash_primitives::transaction::components::orchard::write_v5_bundle;

/// Generate a Unified Address containing an Orchard receiver and a single-action
/// Orchard v5 bundle hex that is recoverable with OVK = 00..00.
pub fn gen_ua_and_orchard_bundle_hex(amount: u64, network: &str) -> (String, String) {
    let mut rng = OsRng;
    // Deterministic-ish recipient based on fixed SpendingKey for reproducibility
    let sk = SpendingKey::from_bytes([7u8; 32]).expect("spending key");
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
        .expect("add output");

    let (unauth, _) = builder
        .build::<zcash_protocol::value::ZatBalance>(&mut rng)
        .expect("build orchard bundle")
        .expect("bundle present");
    let pk = orchard::circuit::ProvingKey::build();
    let authorized = unauth
        .create_proof(&pk, &mut rng)
        .expect("create proof")
        .prepare(&mut rng, [0u8; 32])
        .finalize()
        .expect("finalize proof");

    // Produce Unified Address string containing the Orchard receiver.
    let orchard_raw = recipient.to_raw_address_bytes();
    let ua = zcash_address::unified::Address::try_from_items(vec![Receiver::Orchard(orchard_raw)])
        .expect("UA from orchard receiver");
    let network = match network {
        "main" | "mainnet" => zcash_protocol::consensus::NetworkType::Main,
        _ => zcash_protocol::consensus::NetworkType::Test,
    };
    let ua_str = zcash_address::ZcashAddress::from_unified(network, ua).encode();

    // Serialize bundle to v5 bytes and hex-encode
    let mut bytes = vec![];
    write_v5_bundle(Some(&authorized), &mut bytes).expect("write v5 bundle");
    (ua_str, hex::encode(bytes))
}

