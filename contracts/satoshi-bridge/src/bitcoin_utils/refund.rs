use crate::bitcoin_utils::types::ChainSpecificData;
use crate::env;
use crate::psbt_wrapper::PsbtWrapper;
use crate::{Contract, RefundExecutionInputs};
use near_sdk::PromiseOrValue;

impl Contract {
    /// Default refund gas fee when the caller does not specify one. On Bitcoin the
    /// fee rate is unknown ahead of time, so we charge the configured maximum.
    pub(crate) fn get_refund_gas_fee(&self) -> u128 {
        self.internal_config().max_btc_gas_fee
    }

    /// Execute an approved refund request (Bitcoin). Builds the refund PSBT
    /// synchronously. The caller chooses `timelock_sec` (pass `0` to bypass).
    /// `chain_specific_data` is unused on Bitcoin (kept for a uniform API with
    /// the Zcash variant).
    pub(crate) fn internal_execute_refund(
        &mut self,
        utxo_storage_key: String,
        timelock_sec: u64,
        _chain_specific_data: Option<ChainSpecificData>,
    ) -> PromiseOrValue<()> {
        let refund_request = self.load_refund_request_for_execute(&utxo_storage_key, timelock_sec);
        let RefundExecutionInputs {
            outpoint,
            deposit_output,
            refund_amount,
        } = self.refund_execution_inputs(&refund_request);
        let refund_output = self.build_refund_output(&refund_request.refund_address, refund_amount);

        let mut psbt = PsbtWrapper::new(vec![outpoint], vec![refund_output]);
        psbt.set_input_utxo(vec![deposit_output]);

        let caller = env::predecessor_account_id();
        self.finalize_refund_with_psbt(
            caller,
            refund_request,
            psbt,
            refund_amount,
            utxo_storage_key,
        );
        PromiseOrValue::Value(())
    }
}
