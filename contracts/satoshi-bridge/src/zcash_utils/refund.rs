#![allow(clippy::too_many_arguments)]

use crate::psbt_wrapper::PsbtWrapper;
use crate::zcash_utils::types::ChainSpecificData;
use crate::*;
use near_sdk::{near, require, AccountId};

pub(crate) const GAS_FOR_EXECUTE_REFUND_CALLBACK: Gas = Gas::from_tgas(60);

/// Refund transactions never expire (`expiry_height = 0`).
pub(crate) const REFUND_EXPIRY_HEIGHT: u32 = 0;

impl Contract {
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
