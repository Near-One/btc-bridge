use crate::*;

impl Contract {
    pub fn check_withdraw_rbf_psbt_valid(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        withdraw_rbf_psbt: &Psbt,
    ) -> (u128, u128) {
        let withdraw_change_address_script_pubkey =
            self.internal_config().get_change_script_pubkey();
        let original_tx = original_tx_btc_pending_info.get_transaction();
        let target_address_script_pubkey = original_tx
            .output()
            .iter()
            .find(|v| v.script_pubkey != withdraw_change_address_script_pubkey)
            .cloned()
            .expect("The original tx is not a user withdraw tx.")
            .script_pubkey;
        require!(
            original_tx.output().len() == withdraw_rbf_psbt.unsigned_tx.output.len(),
            "Invalid output num"
        );
        let (_, _, actual_received_amount, gas_fee) = self.check_withdraw_psbt(
            withdraw_rbf_psbt,
            &target_address_script_pubkey,
            &withdraw_change_address_script_pubkey,
            &original_tx_btc_pending_info.vutxos,
            original_tx_btc_pending_info.transfer_amount,
            original_tx_btc_pending_info.withdraw_fee,
        );
        (actual_received_amount, gas_fee)
    }

    pub fn internal_withdraw_rbf(
        &mut self,
        account_id: &AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        #[cfg(feature = "zcash")] expiry_height: u32,
    ) -> String {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        require!(
            &original_tx_btc_pending_info.account_id == account_id,
            "Not allow"
        );
        original_tx_btc_pending_info.assert_not_canceled();
        original_tx_btc_pending_info.assert_withdraw_original_pending_verify_tx();
        let withdraw_rbf_psbt = self
            .generate_psbt_from_original_psbt_and_new_output(original_tx_btc_pending_info, output);

        let mut btc_pending_info = init_rbf_btc_pending_info(
            original_tx_btc_pending_info,
            PendingInfoState::WithdrawUserRbf(RbfState {
                stage: PendingInfoStage::PendingSign,
                original_tx_id: original_btc_pending_verify_id.clone(),
            }),
            #[cfg(feature = "zcash")]
            expiry_height,
        );
        let (actual_received_amount, gas_fee) =
            self.check_withdraw_rbf_psbt_valid(original_tx_btc_pending_info, &withdraw_rbf_psbt);
        btc_pending_info.gas_fee = gas_fee;
        btc_pending_info.actual_received_amount = actual_received_amount;
        btc_pending_info.burn_amount = actual_received_amount + gas_fee;
        // Ensure that the RBF transaction pays more gas than the previous transaction.
        let max_gas_fee = original_tx_btc_pending_info.get_max_gas_fee();
        let additional_gas_amount = gas_fee.saturating_sub(max_gas_fee);
        require!(additional_gas_amount > 0, "No gas increase.");
        self.internal_unwrap_mut_btc_pending_info(&original_btc_pending_verify_id)
            .update_max_gas_fee(gas_fee);
        self.set_rbf_pending_info(
            &original_btc_pending_verify_id,
            btc_pending_info,
            withdraw_rbf_psbt,
            false,
        )
    }
}
