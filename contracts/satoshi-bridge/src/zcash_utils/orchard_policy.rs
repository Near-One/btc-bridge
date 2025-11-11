use near_sdk::require;

use crate::network;

use orchard::Bundle;
use zcash_protocol::value::ZatBalance;

// Bridge OVK used to recover outputs for policy checks.
// For now we hardcode to all zeroes; can be made configurable later.
pub const BRIDGE_OVK: [u8; 32] = [0u8; 32];

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
    use zcash_address::unified::Container;

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

/// Enforce Orchard policy: recovered Orchard amount + miner_fee equals expected_total,
/// and recipient matches the Unified Address's Orchard receiver.
/// Returns the recovered Orchard amount (in zatoshis).
pub fn ensure_orchard_policy(
    bundle: &Bundle<orchard::bundle::Authorized, ZatBalance>,
    target_addr: &str,
    chain: &network::Chain,
    expected_total_outflow: u128,
    miner_fee: u128,
) -> u64 {
    let (orchard_amount, recovered_raw) = recover_orchard_output(bundle);
    let expected_raw = extract_orchard_receiver_from_unified(target_addr, chain);
    require!(recovered_raw == expected_raw, "Orchard recipient mismatch");
    require!(
        orchard_amount as u128 + miner_fee == expected_total_outflow,
        "Orchard+fee totals mismatch"
    );
    orchard_amount
}

/// Verify the Orchard proof for a bundle before entering PendingSign.
/// Feature-gated behind `orchard_proof_verify` to control compute cost.
#[cfg(all(feature = "zcash", feature = "orchard_proof_verify"))]
pub fn verify_orchard_bundle_preflight(
    bundle: &Bundle<orchard::bundle::Authorized, ZatBalance>,
) -> Result<(), &'static str> {
    // Basic sanity: single action for now to bound costs.
    if bundle.actions().len() != 1 {
        return Err("Only one Orchard action is supported");
    }

    // Build VerifyingKey at runtime (Phase 1). This is compute-heavy but
    // avoids a fork; can be replaced later with an embedded VK.
    let vk = orchard::circuit::VerifyingKey::build();

    // Construct public instances for each action.
    let flags = *bundle.flags();
    let anchor = *bundle.anchor();
    let instances = bundle
        .actions()
        .iter()
        .map(|a| a.to_instance(flags, anchor))
        .collect::<Vec<_>>();

    // Verify the Halo2 proof contained in the bundle authorization.
    let auth = bundle.authorization();
    let proof = auth.proof();
    match proof.verify(&vk, &instances) {
        Ok(()) => Ok(()),
        Err(_) => Err("Invalid Orchard proof"),
    }
}
