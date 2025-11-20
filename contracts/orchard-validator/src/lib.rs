use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::{env, near_bindgen, PanicOnDefault};
use std::io::Cursor;
use zcash_primitives::transaction::components::orchard::read_v5_bundle;

/// Hardcoded outgoing viewing key (all zeros for now)
const BRIDGE_OVK: [u8; 32] = [0u8; 32];

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct OrchardValidator {}

#[near_bindgen]
impl OrchardValidator {
    #[init]
    pub fn new() -> Self {
        Self {}
    }

    /// Validates an Orchard bundle:
    /// - Parses the bundle
    /// - Ensures exactly 1 action
    /// - Recovers output with BRIDGE_OVK
    /// - Validates amount matches expected
    ///
    /// Returns the recovered recipient raw bytes (43 bytes) for further validation
    pub fn validate_orchard_bundle(
        bundle_bytes: Vec<u8>,
        expected_amount: u64,
    ) -> Vec<u8> {
        // Parse bundle
        let mut reader = Cursor::new(&bundle_bytes);
        let bundle = read_v5_bundle(&mut reader)
            .expect("Failed to parse Orchard bundle")
            .expect("Orchard bundle is None");

        // Ensure exactly one action
        let action_count = bundle.actions().len();
        if action_count != 1 {
            env::panic_str(&format!(
                "Only one orchard action is supported, found {}",
                action_count
            ));
        }

        // Recover output using BRIDGE_OVK
        let ovk = orchard::keys::OutgoingViewingKey::from(BRIDGE_OVK);
        let (note, recipient_addr, _memo) = bundle
            .recover_output_with_ovk(0, &ovk)
            .expect("Failed to recover Orchard output");

        // Validate amount
        let actual_amount = note.value().inner();
        if actual_amount != expected_amount {
            env::panic_str(&format!(
                "Orchard amount mismatch: expected {}, got {}",
                expected_amount, actual_amount
            ));
        }

        // Return recipient bytes for caller to validate
        recipient_addr.to_raw_address_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_initialization() {
        let _contract = OrchardValidator::new();
    }
}
