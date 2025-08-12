use crate::psbt_wrapper::PsbtWrapper;
use crate::*;

impl Contract {
    pub fn check_active_utxo_management_rbf_psbt_valid(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        active_utxo_management_rbf_psbt: &PsbtWrapper,
    ) -> (u128, u128) {
        let original_tx = original_tx_btc_pending_info.get_transaction();
        require!(
            original_tx.output().len() == active_utxo_management_rbf_psbt.get_output_num(),
            "Invalid output num"
        );
        let (actual_received_amount, gas_fee) = self.check_psbt_output_all_change_address(
            active_utxo_management_rbf_psbt,
            &original_tx_btc_pending_info.vutxos,
            true,
            false,
        );
        (actual_received_amount, gas_fee)
    }

    pub fn internal_active_utxo_management_rbf(
        &mut self,
        account_id: &AccountId,
        original_btc_pending_verify_id: String,
        active_utxo_management_rbf_psbt: PsbtWrapper,
    ) -> String {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        require!(
            &original_tx_btc_pending_info.account_id == account_id,
            "Not allow"
        );
        original_tx_btc_pending_info.assert_not_canceled();
        original_tx_btc_pending_info.assert_active_utxo_management_original_pending_verify_tx();
        let mut btc_pending_info = init_rbf_btc_pending_info(
            original_tx_btc_pending_info,
            PendingInfoState::ActiveUtxoManagementRbf(RbfState {
                stage: PendingInfoStage::PendingSign,
                original_tx_id: original_btc_pending_verify_id.clone(),
            }),
        );
        let (actual_received_amount, gas_fee) = self.check_active_utxo_management_rbf_psbt_valid(
            original_tx_btc_pending_info,
            &active_utxo_management_rbf_psbt,
        );
        btc_pending_info.gas_fee = gas_fee;
        btc_pending_info.burn_amount = gas_fee;
        btc_pending_info.actual_received_amount = actual_received_amount;

        // Ensure that the RBF transaction pays more gas than the previous transaction.
        let max_gas_fee = original_tx_btc_pending_info.get_max_gas_fee();
        let additional_gas_amount = gas_fee.saturating_sub(max_gas_fee);
        require!(additional_gas_amount > 0, "No gas increase.");
        require!(
            self.data().cur_available_protocol_fee >= additional_gas_amount,
            "Insufficient protocol fee"
        );
        self.data_mut().cur_available_protocol_fee -= additional_gas_amount;
        self.data_mut().cur_reserved_protocol_fee += additional_gas_amount;
        self.internal_unwrap_mut_btc_pending_info(&original_btc_pending_verify_id)
            .update_max_gas_fee(gas_fee);
        self.set_rbf_pending_info(
            &original_btc_pending_verify_id,
            btc_pending_info,
            active_utxo_management_rbf_psbt,
            false,
        )
    }
}
