use near_sdk::require;
use orchard::Bundle;
use zcash_address::unified::Container;
use zcash_protocol::value::ZatBalance;

use crate::network;

/// Bridge OVK used to recover outputs for policy checks.
/// Hardcoded to all zeroes for now; can be made configurable later.
pub const BRIDGE_OVK: [u8; 32] = [0u8; 32];

/// Minimum number of actions required in an Orchard bundle per the Orchard protocol.
/// The Orchard builder automatically pads bundles to meet this minimum for privacy.
/// See: https://github.com/zcash/orchard/blob/main/src/builder.rs#L36
pub const MIN_ACTIONS: usize = 2;

/// Recover the Orchard note value and raw address bytes (43 bytes) from the bundle
/// using the bridge OVK. Assumes a single action.
pub fn recover_orchard_output(
    bundle: &Bundle<orchard::bundle::Authorized, ZatBalance>,
) -> (u64, [u8; 43]) {
    let (note, addr, _memo) = bundle
        .recover_output_with_ovk(0, &orchard::keys::OutgoingViewingKey::from(BRIDGE_OVK))
        .expect("Failed to recover Orchard output with bridge OVK");
    (note.value().inner(), addr.to_raw_address_bytes())
}

/// Extract the Orchard receiver raw bytes from a Unified Address string for the given chain.
pub fn extract_orchard_receiver_from_unified(
    target_addr: &str,
    chain: &network::Chain,
) -> [u8; 43] {
    let zaddr = zcash_address::ZcashAddress::try_from_encoded(target_addr)
        .expect("Invalid Zcash address encoding");

    let net = match chain {
        network::Chain::ZcashMainnet => zcash_protocol::consensus::NetworkType::Main,
        network::Chain::ZcashTestnet => zcash_protocol::consensus::NetworkType::Test,
        _ => near_sdk::env::panic_str("Unsupported chain for Orchard withdraw"),
    };

    let local_addr: network::Address = zaddr
        .convert_if_network::<network::Address>(net)
        .expect("Address network mismatch");

    let ua = match local_addr {
        network::Address::Unified { address, .. } => address,
        _ => near_sdk::env::panic_str("Expected Unified Zcash address for Orchard withdraw"),
    };

    for recv in ua.items_as_parsed() {
        if let zcash_address::unified::Receiver::Orchard(bytes) = recv {
            return *bytes;
        }
    }
    near_sdk::env::panic_str("Unified address missing Orchard receiver")
}

/// Validate Orchard bundle against policy:
/// - Recovers the output using BRIDGE_OVK
/// - Verifies the recovered amount matches expected
/// - Verifies the recovered recipient matches the expected Unified Address's Orchard receiver
pub fn validate_orchard_bundle(
    bundle: &Bundle<orchard::bundle::Authorized, ZatBalance>,
    expected_recipient: &str,
    expected_amount: u64,
    chain: &network::Chain,
) {
    // Enforce minimum actions per Orchard protocol
    require!(
        bundle.actions().len() >= MIN_ACTIONS,
        format!(
            "Orchard bundle must have at least {} actions, got {}",
            MIN_ACTIONS,
            bundle.actions().len()
        )
    );

    // Recover output with bridge OVK
    let (recovered_amount, recovered_addr_bytes) = recover_orchard_output(bundle);

    // Validate amount
    require!(
        recovered_amount == expected_amount,
        format!(
            "Orchard amount mismatch: expected {}, got {}",
            expected_amount, recovered_amount
        )
    );

    // Validate recipient
    let expected_addr_bytes = extract_orchard_receiver_from_unified(expected_recipient, chain);
    require!(
        recovered_addr_bytes == expected_addr_bytes,
        format!(
            "Orchard recipient mismatch: expected {} does not match recovered output",
            expected_recipient
        )
    );
}
