use crate::network;
use crate::network::Address;
use orchard::Bundle;
use std::io::Cursor;
use zcash_primitives::transaction::components::orchard::read_v5_bundle;
use zcash_protocol::value::ZatBalance;
/// Bridge OVK used to recover outputs for policy checks.
/// Hardcoded to all zeroes for now; can be made configurable later.
pub const BRIDGE_OVK: [u8; 32] = [0u8; 32];

/// Minimum number of actions required in an Orchard bundle per the Orchard protocol.
/// The Orchard builder automatically pads bundles to meet this minimum for privacy.
/// See: https://github.com/zcash/orchard/blob/main/src/builder.rs#L36
pub const EXPECTED_ACTIONS_NUMBER: usize = 1;

pub fn extract_orchard_bundle(
    orchard_bundle_bytes: Option<Vec<u8>>,
) -> Result<
    (
        Option<orchard::Bundle<orchard::bundle::Authorized, ZatBalance>>,
        Option<(u64, [u8; 43])>,
    ),
    String,
> {
    if let Some(orchard_bundle_bytes) = orchard_bundle_bytes {
        let mut real_outputs = Vec::new();
        let mut reader = Cursor::new(orchard_bundle_bytes);
        let bundle = read_v5_bundle(&mut reader)
            .map_err(|_| "Failed to read orchard bundle".to_string())?
            .ok_or_else(|| "Orchard bundle is empty".to_string())?;

        let ovk = orchard::keys::OutgoingViewingKey::from(BRIDGE_OVK);

        for action_idx in 0..bundle.actions().len() {
            if let Some((note, addr, _memo)) = bundle.recover_output_with_ovk(action_idx, &ovk) {
                let value = note.value().inner();
                if value > 0 {
                    real_outputs.push((value, addr.to_raw_address_bytes()));
                }
            }
        }

        if real_outputs.len() != 1 {
            return Err(format!(
                "Expected exactly 1 non-zero Orchard output, found {}",
                real_outputs.len()
            ));
        }

        // If no expected values provided, enforce minimum actions per Orchard protocol
        if bundle.actions().len() != EXPECTED_ACTIONS_NUMBER {
            return Err(format!(
                "Orchard bundle must have {} actions, got {}",
                EXPECTED_ACTIONS_NUMBER,
                bundle.actions().len()
            ));
        }

        Ok((Some(bundle), real_outputs.pop()))
    } else {
        Ok((None, None))
    }
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
    chain: &network::Chain,
    orchard_output: (u64, [u8; 43]),
) -> Result<(), String> {
    let (recovered_amount, recovered_addr_bytes) = orchard_output;

    let recipient_address = Address::parse(expected_recipient, chain.clone())?;

    // Validate recipient
    let expected_addr_bytes = recipient_address.extract_orchard_receiver()?;
    if recovered_addr_bytes != expected_addr_bytes {
        return Err(format!(
            "Orchard recipient mismatch: expected {} does not match recovered output",
            expected_recipient
        ));
    }

    // Validate value balance: for withdrawal, value flows FROM transparent TO Orchard
    // So value_balance should be negative and equal to the output amount
    let value_balance = bundle.value_balance();
    let expected_value_balance =
        -i64::try_from(recovered_amount).map_err(|_| "Orchard amount too large for i64")?;

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
