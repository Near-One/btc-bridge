use orchard::builder::{Builder, BundleType};
use orchard::bundle::{BundleVersion, Flags};
use orchard::keys::{FullViewingKey, OutgoingViewingKey, Scope, SpendingKey};
use orchard::tree::Anchor;
use orchard::value::NoteValue;
use rand::rngs::OsRng;
use zcash_address::unified::{Encoding, Receiver};
use zcash_address::{ToAddress, ZcashAddress};
use zcash_primitives::transaction::components::orchard::{write_v5_bundle, write_v6_bundle};

/// Bridge OVK used for all test bundles (same as production)
pub const BRIDGE_OVK: [u8; 32] = [0u8; 32];

/// Generate a Unified Address containing an Orchard receiver and a single-action
/// shielded bundle hex (for the pool selected by `bundle_version`) that is
/// recoverable with BRIDGE_OVK.
///
/// This function is expensive (generates Halo2 proof), so results should be cached.
///
/// If spending_key_bytes is provided, uses that key; otherwise defaults to [7u8; 32].
pub fn gen_ua_and_bundle_hex_with_key_version(
    amount: u64,
    network: &str,
    spending_key_bytes: Option<[u8; 32]>,
    bundle_version: BundleVersion,
) -> (String, String) {
    let mut rng = OsRng;

    // Use provided spending key or default to [7u8; 32] for test reproducibility
    let sk_bytes = spending_key_bytes.unwrap_or([7u8; 32]);
    let sk = SpendingKey::from_bytes(sk_bytes).expect("spending key");
    let fvk = FullViewingKey::from(&sk);
    let recipient = fvk.address_at(0u32, Scope::External);

    // Build a simple output-only bundle with BRIDGE_OVK
    // Use Coinbase bundle type which supports single output without dummy actions.
    let mut builder = Builder::new(
        BundleType::Coinbase,
        bundle_version,
        Flags::SPENDS_DISABLED,
        Anchor::empty_tree(),
    )
    .expect("orchard builder");
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

    let pk = orchard::circuit::ProvingKey::build(bundle_version.circuit_version());
    let authorized = unauth
        .create_proof(&pk, &mut rng)
        .expect("create proof")
        .prepare(rng, [0u8; 32])
        .finalize()
        .expect("finalize proof");

    // Produce Unified Address string containing BOTH Orchard and transparent (P2PKH) receivers
    // The transparent receiver is derived from the spending key for consistency
    let orchard_raw = recipient.to_raw_address_bytes();

    // Generate a deterministic P2PKH hash from the spending key
    // This allows the contract to extract a script_pubkey for validation
    let mut p2pkh_hash = [0u8; 20];
    p2pkh_hash.copy_from_slice(&sk_bytes[0..20]);

    let ua = zcash_address::unified::Address::try_from_items(vec![
        Receiver::Orchard(orchard_raw),
        Receiver::P2pkh(p2pkh_hash),
    ])
    .expect("UA from orchard and p2pkh receivers");

    let network_type = match network {
        "main" | "mainnet" => zcash_protocol::consensus::NetworkType::Main,
        _ => zcash_protocol::consensus::NetworkType::Test,
    };
    let zaddr = ZcashAddress::from_unified(network_type, ua);
    let ua_str = zaddr.encode();

    // Serialize the bare bundle and hex-encode. The wire format is shared; the
    // v6 writer additionally guards that the bundle version fits a v6 slot.
    let mut bytes = vec![];
    if bundle_version == BundleVersion::ironwood_v3() {
        write_v6_bundle(Some(&authorized), &mut bytes).expect("write v6 bundle");
    } else {
        write_v5_bundle(Some(&authorized), &mut bytes).expect("write v5 bundle");
    }

    (ua_str, hex::encode(bytes))
}

/// Legacy-Orchard variant (pre-NU6.3 epochs): orchard_v2 keeps the pre-NU6.3 wire
/// format (V2 notes, fixed post-NU6.2 circuit), matching the heights most
/// integration tests run at and the checked-in cached fixtures.
pub fn gen_ua_and_orchard_bundle_hex_with_key(
    amount: u64,
    network: &str,
    spending_key_bytes: Option<[u8; 32]>,
) -> (String, String) {
    gen_ua_and_bundle_hex_with_key_version(
        amount,
        network,
        spending_key_bytes,
        BundleVersion::orchard_v2(),
    )
}

/// Generate a Unified Address and bundle with default spending key [7u8; 32].
/// Wrapper for backward compatibility.
pub fn gen_ua_and_orchard_bundle_hex(amount: u64, network: &str) -> (String, String) {
    gen_ua_and_orchard_bundle_hex_with_key(amount, network, None)
}

/// Produce a Unified Address containing ONLY an Orchard receiver (no transparent receiver),
/// derived from `spending_key_bytes` (default `[7u8; 32]`). This reproduces a shielded-only
/// recipient like the ones privacy wallets emit. It is cheap (derives the address only, no
/// Halo2 proof), and the Orchard receiver matches the bundle produced by
/// `gen_ua_and_orchard_bundle_hex*` for the same key, so a cached bundle can be reused as the
/// payment.
pub fn orchard_only_ua_with_key(network: &str, spending_key_bytes: Option<[u8; 32]>) -> String {
    let sk_bytes = spending_key_bytes.unwrap_or([7u8; 32]);
    let sk = SpendingKey::from_bytes(sk_bytes).expect("spending key");
    let fvk = FullViewingKey::from(&sk);
    let recipient = fvk.address_at(0u32, Scope::External);
    let orchard_raw = recipient.to_raw_address_bytes();

    let ua = zcash_address::unified::Address::try_from_items(vec![Receiver::Orchard(orchard_raw)])
        .expect("UA from orchard receiver");

    let network_type = match network {
        "main" | "mainnet" => zcash_protocol::consensus::NetworkType::Main,
        _ => zcash_protocol::consensus::NetworkType::Test,
    };
    ZcashAddress::from_unified(network_type, ua).encode()
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
    println!(
        "Generating Orchard bundle for amount {}... (this may take a while)",
        amount
    );
    let (ua, bundle_hex) = gen_ua_and_orchard_bundle_hex(amount, "testnet");

    // Save to cache
    let cache_content = format!("{}\n{}", ua, bundle_hex);
    if let Err(e) = fs::write(cache_path, cache_content) {
        eprintln!("Warning: Failed to cache bundle: {}", e);
    }

    (ua, bundle_hex)
}

/// Get or generate a cached Ironwood (NU6.3, `BundleVersion::ironwood_v3`) bundle
/// for the given amount: V3 note plaintexts (lead byte 0x03), post-NU6.3 circuit.
/// The recipient UA is identical to the legacy fixtures for the same key — NU6.3
/// does not change address encodings, only the pool the funds land in.
pub fn get_or_gen_ironwood_bundle(amount: u64) -> (String, String) {
    use std::fs;
    use std::path::Path;

    let cache_file = format!("tests/orchard_bundle_cache_ironwood_{}.txt", amount);
    let cache_path = Path::new(&cache_file);

    if cache_path.exists() {
        if let Ok(contents) = fs::read_to_string(cache_path) {
            let lines: Vec<&str> = contents.lines().collect();
            if lines.len() == 2 {
                return (lines[0].to_string(), lines[1].to_string());
            }
        }
    }

    println!(
        "Generating Ironwood bundle for amount {}... (this may take a while)",
        amount
    );
    let (ua, bundle_hex) = gen_ua_and_bundle_hex_with_key_version(
        amount,
        "testnet",
        None,
        BundleVersion::ironwood_v3(),
    );

    let cache_content = format!("{}\n{}", ua, bundle_hex);
    if let Err(e) = fs::write(cache_path, cache_content) {
        eprintln!("Warning: Failed to cache bundle: {}", e);
    }

    (ua, bundle_hex)
}

/// Generate a bundle with a specific spending key (for testing recipient mismatch).
/// Caches based on both amount and spending key to avoid expensive regeneration.
pub fn gen_bundle_with_key(amount: u64, spending_key: [u8; 32]) -> (String, String) {
    use std::fs;
    use std::path::Path;

    // Create cache key from amount + hex-encoded spending key
    let key_hex = hex::encode(spending_key);
    let cache_file = format!("tests/orchard_bundle_cache_{}_{}.txt", amount, key_hex);
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
    println!(
        "Generating Orchard bundle with custom key for amount {}... (this may take a while)",
        amount
    );
    let (ua, bundle_hex) =
        gen_ua_and_orchard_bundle_hex_with_key(amount, "testnet", Some(spending_key));

    // Save to cache
    let cache_content = format!("{}\n{}", ua, bundle_hex);
    if let Err(e) = fs::write(cache_path, cache_content) {
        eprintln!("Warning: Failed to cache bundle: {}", e);
    }

    (ua, bundle_hex)
}
