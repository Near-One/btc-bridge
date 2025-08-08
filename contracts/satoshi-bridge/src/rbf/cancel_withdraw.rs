use crate::*;

impl Contract {
    pub fn check_cancel_withdraw_rbf_psbt_valid(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        cancel_withdraw_rbf_psbt: &Psbt,
    ) -> (u128, u128) {
        let (actual_received_amount, gas_fee) = self.check_psbt_output_all_change_address(
            cancel_withdraw_rbf_psbt,
            &original_tx_btc_pending_info.vutxos,
            false,
            true,
        );
        (actual_received_amount, gas_fee)
    }

    pub fn internal_cancel_withdraw(
        &mut self,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        #[cfg(feature = "zcash")] expiry_height: u32,
    ) -> String {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        require!(
            nano_to_sec(env::block_timestamp()) - original_tx_btc_pending_info.create_time_sec
                > self.internal_config().max_btc_tx_pending_sec,
            "Please wait user rbf"
        );
        original_tx_btc_pending_info.assert_not_canceled();
        original_tx_btc_pending_info.assert_withdraw_original_pending_verify_tx();
        let cancel_withdraw_rbf_psbt = self
            .generate_psbt_from_original_psbt_and_new_output(original_tx_btc_pending_info, output);

        let mut btc_pending_info = init_rbf_btc_pending_info(
            original_tx_btc_pending_info,
            PendingInfoState::WithdrawCancelRbf(RbfState {
                stage: PendingInfoStage::PendingSign,
                original_tx_id: original_btc_pending_verify_id.clone(),
            }),
            #[cfg(feature = "zcash")]
            expiry_height,
        );
        let (actual_received_amount, gas_fee) = self.check_cancel_withdraw_rbf_psbt_valid(
            original_tx_btc_pending_info,
            &cancel_withdraw_rbf_psbt.psbt,
        );

        btc_pending_info.gas_fee = gas_fee;
        btc_pending_info.burn_amount = gas_fee;
        btc_pending_info.actual_received_amount = actual_received_amount;
        #[cfg(not(feature = "zcash"))]
        {
            // Ensure that the RBF transaction pays more gas than the previous transaction.
            let max_gas_fee = original_tx_btc_pending_info.get_max_gas_fee();
            let additional_gas_amount = gas_fee.saturating_sub(max_gas_fee);
            require!(additional_gas_amount > 0, "No gas increase.");
        }
        let excess_gas_fee = gas_fee
            .saturating_sub(btc_pending_info.transfer_amount - btc_pending_info.withdraw_fee);
        if excess_gas_fee > 0 {
            require!(
                self.acl_has_role(Role::DAO.into(), env::predecessor_account_id()),
                "gas fee exceeds the user's balance, only the owner is allowed to cancel"
            );
            require!(
                self.data().cur_available_protocol_fee >= excess_gas_fee,
                "Insufficient protocol fee"
            );
            self.data_mut().cur_available_protocol_fee -= excess_gas_fee;
            self.data_mut().cur_reserved_protocol_fee += excess_gas_fee;
        }
        self.internal_unwrap_mut_btc_pending_info(&original_btc_pending_verify_id)
            .do_cancel(gas_fee, excess_gas_fee);
        self.set_rbf_pending_info(
            &original_btc_pending_verify_id,
            btc_pending_info,
            cancel_withdraw_rbf_psbt.psbt,
            true,
        )
    }
}
