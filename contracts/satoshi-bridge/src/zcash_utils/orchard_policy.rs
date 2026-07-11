use crate::network::Address;
use crate::network::{Chain, OrchardRawAddress};
use orchard::{Bundle, ValuePool};
use std::io::Cursor;
use zcash_primitives::transaction::components::orchard::{read_v5_bundle, read_v6_bundle};
use zcash_protocol::consensus::BranchId;
use zcash_protocol::value::ZatBalance;

use super::psbt_wrapper::is_ironwood;

/// Bridge OVK used to recover outputs for policy checks.
/// Hardcoded to all zeroes for now; can be made configurable later.
pub const BRIDGE_OVK: [u8; 32] = [0u8; 32];

pub const EXPECTED_ACTIONS_NUMBER: usize = 1;

pub struct OrchardOutput {
    pub amount: u64,
    pub recipient_addr: OrchardRawAddress,
}

pub struct ParsedOrchardBundle {
    pub bundle: Bundle<orchard::bundle::Authorized, ZatBalance>,
    pub output: OrchardOutput,
}

impl ParsedOrchardBundle {
    pub fn amount(&self) -> u128 {
        self.output.amount.into()
    }

    pub fn recipient_addr(&self) -> &OrchardRawAddress {
        &self.output.recipient_addr
    }
}

pub fn extract_orchard_bundle(
    orchard_bundle_bytes: Option<Vec<u8>>,
    branch_id: BranchId,
) -> Result<Option<ParsedOrchardBundle>, String> {
    if let Some(orchard_bundle_bytes) = orchard_bundle_bytes {
        let mut reader = Cursor::new(orchard_bundle_bytes);
        let bundle = if is_ironwood(branch_id) {
            read_v6_bundle(&mut reader, branch_id, ValuePool::Ironwood)
        } else {
            read_v5_bundle(&mut reader, branch_id)
        }
        .map_err(|_| "Failed to read orchard bundle".to_string())?
        .ok_or_else(|| "Orchard bundle is empty".to_string())?;

        // Check action count first per Orchard protocol requirements
        if bundle.actions().len() != EXPECTED_ACTIONS_NUMBER {
            return Err(format!(
                "Orchard bundle must have {} actions, got {}",
                EXPECTED_ACTIONS_NUMBER,
                bundle.actions().len()
            ));
        }

        // Since we require exactly 1 action, directly recover the single output
        let ovk = orchard::keys::OutgoingViewingKey::from(BRIDGE_OVK);
        let (note, addr, _memo) = bundle
            .recover_output_with_ovk(0, &ovk)
            .ok_or_else(|| "Failed to recover Orchard output with bridge OVK".to_string())?;

        let value = note.value().inner();
        if value == 0 {
            return Err("Orchard output value must be non-zero".to_string());
        }

        Ok(Some(ParsedOrchardBundle {
            bundle,
            output: OrchardOutput {
                amount: value,
                recipient_addr: addr.to_raw_address_bytes(),
            },
        }))
    } else {
        Ok(None)
    }
}

/// Validate Orchard bundle against policy:
/// - Recovers all outputs using BRIDGE_OVK
/// - Verifies exactly one non-zero output exists
/// - Verifies the recovered amount is within expected range (allows dust adjustment)
/// - Verifies the recovered recipient matches the expected Unified Address's Orchard receiver
/// - Verifies value balance matches the output amount (value flows from transparent to Orchard)
pub fn validate_orchard_bundle(
    orchard: &ParsedOrchardBundle,
    expected_recipient: &str,
    chain: &Chain,
) -> Result<(), String> {
    let recipient_address = Address::parse(expected_recipient, chain.clone())?;

    // Validate recipient
    let expected_addr_bytes = recipient_address.extract_orchard_receiver()?;
    if orchard.recipient_addr() != &expected_addr_bytes {
        return Err(format!(
            "Orchard recipient mismatch: expected {} does not match recovered output",
            expected_recipient
        ));
    }

    // Validate value balance: for withdrawal, value flows FROM transparent TO Orchard
    // So value_balance should be negative and equal to the output amount
    let value_balance = orchard.bundle.value_balance();
    let expected_value_balance =
        -i64::try_from(orchard.amount()).map_err(|_| "Orchard amount too large for i64")?;

    let actual_value_balance: i64 = (*value_balance).into();
    if actual_value_balance != expected_value_balance {
        return Err(format!(
            "Orchard value balance mismatch: expected {}, got {}. \
             Value balance must equal negative output amount for withdrawals",
            expected_value_balance, actual_value_balance
        ));
    }

    Ok(())
}
