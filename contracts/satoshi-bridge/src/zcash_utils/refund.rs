#![allow(clippy::too_many_arguments)]

use crate::psbt_wrapper::{zip317_min_fee, PsbtWrapper};
use crate::zcash_utils::orchard_policy::EXPECTED_ACTIONS_NUMBER;
use crate::zcash_utils::types::ChainSpecificData;
use crate::*;
use near_sdk::{near, require, AccountId};
use zcash_primitives::transaction::fees::zip317::P2PKH_STANDARD_OUTPUT_SIZE;

pub(crate) const GAS_FOR_EXECUTE_REFUND_CALLBACK: Gas = Gas::from_tgas(60);

/// Refund transactions never expire (`expiry_height = 0`).
pub(crate) const REFUND_EXPIRY_HEIGHT: u32 = 0;

impl Contract {
    /// Default refund gas fee when the caller does not specify one. A refund spends
    /// one input and is either transparent (one P2PKH output) or shielded (one
    /// Orchard action); charge the larger ZIP-317 minimum so either form is covered.
    pub(crate) fn get_refund_gas_fee(&self) -> u128 {
        transparent_refund_min_fee().max(shielded_refund_min_fee())
    }

    /// Execute an approved refund request (Zcash). Building a Zcash transaction
    /// requires the current block height (for the consensus `branch_id`), so this
    /// fetches it asynchronously and finishes in `execute_refund_callback`.
    /// When `chain_specific_data` carries an Orchard bundle the refund is shielded;
    /// otherwise it is a transparent refund to `refund_address`.
    pub(crate) fn internal_execute_refund(
        &mut self,
        utxo_storage_key: String,
        timelock_sec: u64,
        chain_specific_data: Option<ChainSpecificData>,
    ) -> PromiseOrValue<()> {
        let caller = env::predecessor_account_id();
        PromiseOrValue::Promise(
            self.get_last_block_height_promise().then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_EXECUTE_REFUND_CALLBACK)
                    .execute_refund_callback(
                        utxo_storage_key,
                        caller,
                        timelock_sec,
                        chain_specific_data,
                    ),
            ),
        )
    }
}

/// ZIP-317 minimum for a transparent refund: one input, one standard P2PKH output.
fn transparent_refund_min_fee() -> u128 {
    zip317_min_fee(1, vec![P2PKH_STANDARD_OUTPUT_SIZE], 0, 0).into_u64() as u128
}

/// ZIP-317 minimum for a shielded refund: one input, no transparent output, and the
/// fixed `EXPECTED_ACTIONS_NUMBER` shielded actions. ZIP-317 rev.1 sums Orchard
/// and Ironwood action counts, so the numeric fee is the same whether the
/// refund is routed through the Orchard slot (pre-NU6.3) or the Ironwood slot
/// (NU6.3+); we account for it in the Ironwood slot which covers both epochs.
fn shielded_refund_min_fee() -> u128 {
    zip317_min_fee(1, vec![], 0, EXPECTED_ACTIONS_NUMBER).into_u64() as u128
}

#[cfg(test)]
mod refund_gas_fee_tests {
    use super::{shielded_refund_min_fee, transparent_refund_min_fee};

    #[test]
    fn refund_gas_fee_components() {
        // 1 input, 1 P2PKH output, 0 Orchard actions:
        // logical = max(ceil(150/150), ceil(34/34)) = 1 → 5000 * max(2, 1) = 10000.
        assert_eq!(transparent_refund_min_fee(), 10_000);

        // 1 input, 0 transparent outputs, EXPECTED_ACTIONS_NUMBER Orchard actions:
        // logical = 1 + EXPECTED_ACTIONS_NUMBER → 5000 * max(2, logical).
        assert_eq!(shielded_refund_min_fee(), 10_000);
    }
}

#[near]
impl Contract {
    #[private]
    pub fn execute_refund_callback(
        &mut self,
        utxo_storage_key: String,
        caller: AccountId,
        timelock_sec: u64,
        chain_specific_data: Option<ChainSpecificData>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        // Enforce the timelock and that the UTXO has not been finalized via deposit.
        let refund_request = self.load_refund_request_for_execute(&utxo_storage_key, timelock_sec);
        let RefundExecutionInputs {
            outpoint,
            deposit_output,
            refund_amount,
        } = self.refund_execution_inputs(&refund_request);

        let expiry_height = REFUND_EXPIRY_HEIGHT;
        let orchard_bundle = chain_specific_data.map(|c| c.orchard_bundle_bytes.0);

        // Shielded refund routes funds through the Orchard bundle (no transparent
        // output); transparent refund pays a single t-address output.
        let output = if orchard_bundle.is_some() {
            Vec::new()
        } else {
            vec![self.build_refund_output(&refund_request.refund_address, refund_amount)]
        };

        let mut psbt = PsbtWrapper::new(
            vec![outpoint],
            output,
            orchard_bundle,
            expiry_height,
            last_block_height,
            Some(refund_request.refund_address.clone()),
            self.internal_config(),
        );
        psbt.set_input_utxo(vec![deposit_output]);

        // Validate the gas fee covers the Zcash minimum and, for shielded refunds,
        // that the Orchard bundle pays out to `refund_address`.
        self.check_psbt_chain_specific(
            &psbt,
            refund_request.gas_fee,
            refund_request.refund_address.clone(),
        );

        // `validate_orchard_bundle` only checks the recipient and the bundle's
        // internal value balance, not that it matches the deposit economics.
        // Enforce that the shielded output equals deposit - gas, otherwise the
        // resulting transaction would not balance against the chosen gas fee.
        if psbt.has_orchard_bundle() {
            require!(
                psbt.get_orchard_output_amount() == refund_amount,
                format!(
                    "Orchard output amount ({}) does not match refund amount ({})",
                    psbt.get_orchard_output_amount(),
                    refund_amount
                )
            );
        }

        self.finalize_refund_with_psbt(
            caller,
            refund_request,
            psbt,
            refund_amount,
            utxo_storage_key,
        );
    }
}
