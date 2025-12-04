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
pub const MIN_ACTIONS: usize = 1;

/// Check if a Unified Address contains an Orchard receiver.
/// Returns true if the address is a Unified Address with an Orchard receiver.
pub fn has_orchard_receiver(target_addr: &str, chain: &network::Chain) -> bool {
    // Try to parse as Zcash address
    let zaddr = match zcash_address::ZcashAddress::try_from_encoded(target_addr) {
        Ok(addr) => addr,
        Err(_) => return false, // Not a valid Zcash address
    };

    let net = match chain {
        network::Chain::ZcashMainnet => zcash_protocol::consensus::NetworkType::Main,
        network::Chain::ZcashTestnet => zcash_protocol::consensus::NetworkType::Test,
        _ => return false, // Not a ZCash chain
    };

    let local_addr: network::Address = match zaddr.convert_if_network::<network::Address>(net) {
        Ok(addr) => addr,
        Err(_) => return false, // Network mismatch
    };

    // Check if it's a Unified Address
    let ua = match local_addr {
        network::Address::Unified { address, .. } => address,
        _ => return false, // Not a Unified Address
    };

    // Check for Orchard receiver
    for recv in ua.items_as_parsed() {
        if let zcash_address::unified::Receiver::Orchard(_) = recv {
            return true;
        }
    }
    false
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
/// - Recovers all outputs using BRIDGE_OVK
/// - Verifies exactly one non-zero output exists
/// - Verifies the recovered amount is within expected range (allows dust adjustment)
/// - Verifies the recovered recipient matches the expected Unified Address's Orchard receiver
/// - Verifies value balance matches the output amount (value flows from transparent to Orchard)
pub fn validate_orchard_bundle(
    bundle: &Bundle<orchard::bundle::Authorized, ZatBalance>,
    expected_recipient: &str,
    expected_amount: u64,
    min_change_amount: u64,
    chain: &network::Chain,
    real_outputs: Vec<(u64, [u8; 43])>,
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

    require!(
        real_outputs.len() == 1,
        format!(
            "Expected exactly 1 non-zero Orchard output, found {}",
            real_outputs.len()
        )
    );

    let (recovered_amount, recovered_addr_bytes) = real_outputs[0];

    // Validate amount is within range (allow dust adjustment like transparent validation)
    let expected_max = expected_amount;
    let expected_min = expected_amount.saturating_sub(min_change_amount);

    require!(
        recovered_amount >= expected_min && recovered_amount <= expected_max,
        format!(
            "Orchard amount {} out of valid range ({}, {})",
            recovered_amount, expected_min, expected_max
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

    // Validate value balance: for withdrawal, value flows FROM transparent TO Orchard
    // So value_balance should be negative and equal to the output amount
    let value_balance = bundle.value_balance();
    let expected_value_balance =
        -i64::try_from(recovered_amount).expect("Orchard amount too large for i64");

    let actual_value_balance: i64 = (*value_balance).into();
    require!(
        actual_value_balance == expected_value_balance,
        format!(
            "Orchard value balance mismatch: expected {}, got {}. \
             Value balance must equal negative output amount for withdrawals",
            expected_value_balance, actual_value_balance
        )
    );
}
