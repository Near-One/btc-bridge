#![cfg(feature = "zcash")]

use orchard::builder::{Builder, BundleType};
use orchard::keys::{FullViewingKey, OutgoingViewingKey, Scope, SpendingKey};
use orchard::tree::Anchor;
use orchard::value::NoteValue;
use rand::rngs::OsRng;
use zcash_address::unified::{Encoding, Receiver};
use zcash_address::{ToAddress, ZcashAddress};
use zcash_primitives::transaction::components::orchard::write_v5_bundle;

/// Bridge OVK used for all test bundles (same as production)
pub const BRIDGE_OVK: [u8; 32] = [0u8; 32];

/// Generate a Unified Address containing an Orchard receiver and a single-action
/// Orchard v5 bundle hex that is recoverable with BRIDGE_OVK.
///
/// This function is expensive (generates Halo2 proof), so results should be cached.
pub fn gen_ua_and_orchard_bundle_hex(amount: u64, network: &str) -> (String, String) {
    let mut rng = OsRng;

    // Deterministic recipient based on fixed SpendingKey for test reproducibility
    let sk = SpendingKey::from_bytes([7u8; 32]).expect("spending key");
    let fvk = FullViewingKey::from(&sk);
    let recipient = fvk.address_at(0u32, Scope::External);

    // Build a simple output-only bundle with BRIDGE_OVK
    let mut builder = Builder::new(BundleType::DEFAULT, Anchor::empty_tree());
    builder
        .add_output(
            Some(OutgoingViewingKey::from(BRIDGE_OVK)),
            recipient,
            NoteValue::from_raw(amount),
            [0u8; 512], // memo
        )
        .expect("add output");

    // Build and authorize the bundle (this is the expensive part - generates Halo2 proof)
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

    // Produce Unified Address string containing the Orchard receiver
    let orchard_raw = recipient.to_raw_address_bytes();
    let ua = zcash_address::unified::Address::try_from_items(vec![Receiver::Orchard(orchard_raw)])
        .expect("UA from orchard receiver");

    let network_type = match network {
        "main" | "mainnet" => zcash_protocol::consensus::NetworkType::Main,
        _ => zcash_protocol::consensus::NetworkType::Test,
    };
    let zaddr = ZcashAddress::from_unified(network_type, ua);
    let ua_str = zaddr.encode();

    // Serialize bundle to v5 bytes and hex-encode
    let mut bytes = vec![];
    write_v5_bundle(Some(&authorized), &mut bytes).expect("write v5 bundle");

    (ua_str, hex::encode(bytes))
}

/// Get or generate a cached Orchard bundle for the given amount.
/// Caches to a local file to avoid expensive regeneration.
pub fn get_or_gen_bundle(amount: u64) -> (String, String) {
    use std::fs;
    use std::path::Path;

    let cache_file = format!("tests/orchard_bundle_cache_{}.txt", amount);
    let cache_path = Path::new(&cache_file);

    // Try to load from cache
    if cache_path.exists() {
        if let Ok(contents) = fs::read_to_string(cache_path) {
            let lines: Vec<&str> = contents.lines().collect();
            if lines.len() == 2 {
                return (lines[0].to_string(), lines[1].to_string());
            }
        }
    }

    // Cache miss or invalid - generate new bundle
    println!("Generating Orchard bundle for amount {}... (this may take a while)", amount);
    let (ua, bundle_hex) = gen_ua_and_orchard_bundle_hex(amount, "testnet");

    // Save to cache
    let cache_content = format!("{}\n{}", ua, bundle_hex);
    if let Err(e) = fs::write(cache_path, cache_content) {
        eprintln!("Warning: Failed to cache bundle: {}", e);
    }

    (ua, bundle_hex)
}
