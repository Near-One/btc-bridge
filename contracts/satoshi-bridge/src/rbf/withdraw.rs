use crate::{
    init_rbf_btc_pending_info, network::Address, psbt_wrapper::PsbtWrapper, require, AccountId,
    BTCPendingInfo, Contract, PendingInfoStage, PendingInfoState, RbfState,
};

impl Contract {
    pub fn check_withdraw_rbf_psbt_valid(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        withdraw_rbf_psbt: &PsbtWrapper,
        subsidy_amount: u128,
    ) -> (u128, u128) {
        let withdraw_change_address_script_pubkey =
            self.internal_config().get_change_script_pubkey();
        let original_tx =
            original_tx_btc_pending_info.get_transaction(&self.internal_config().chain);
        require!(
            original_tx.output().len() == withdraw_rbf_psbt.get_output_num(),
            "Invalid output num"
        );

        let target_address = self.extract_recipient_address(original_tx_btc_pending_info);

        let (_, _, actual_received_amount, gas_fee) = self.check_withdraw_psbt(
            withdraw_rbf_psbt,
            target_address,
            &withdraw_change_address_script_pubkey,
            &original_tx_btc_pending_info.vutxos,
            original_tx_btc_pending_info.transfer_amount + subsidy_amount,
            original_tx_btc_pending_info.withdraw_fee,
        );
        (actual_received_amount, gas_fee)
    }

    pub fn internal_withdraw_rbf(
        &mut self,
        account_id: &AccountId,
        original_btc_pending_verify_id: String,
        withdraw_rbf_psbt: PsbtWrapper,
        _predecessor_account_id: AccountId,
    ) -> String {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        require!(
            &original_tx_btc_pending_info.account_id == account_id,
            "Not allow"
        );
        original_tx_btc_pending_info.assert_not_canceled();
        original_tx_btc_pending_info.assert_withdraw_original_pending_verify_tx();

        let mut btc_pending_info = init_rbf_btc_pending_info(
            original_tx_btc_pending_info,
            PendingInfoState::WithdrawUserRbf(RbfState {
                stage: PendingInfoStage::PendingSign,
                original_tx_id: original_btc_pending_verify_id.clone(),
            }),
        );
        
        let full_subsidy_amount = self
            .internal_unwrap_btc_pending_info(&original_btc_pending_verify_id)
            .get_subsidize_amount();
        btc_pending_info.transfer_amount += full_subsidy_amount;

        let (actual_received_amount, gas_fee) = self.check_withdraw_rbf_psbt_valid(
            original_tx_btc_pending_info,
            &withdraw_rbf_psbt,
            full_subsidy_amount,
        );
 
        btc_pending_info.gas_fee = gas_fee;
        btc_pending_info.actual_received_amount = actual_received_amount;
        btc_pending_info.burn_amount = actual_received_amount + gas_fee;
        Self::check_withdraw_chain_specific(original_tx_btc_pending_info, gas_fee);
        
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

impl Contract {
    fn extract_recipient_address(&self, original_tx_btc_pending_info: &BTCPendingInfo) -> String {
        let psbt = original_tx_btc_pending_info.get_psbt();

        if let Some(recipient) = psbt.get_recipient_address().clone() {
            return recipient;
        }

        let withdraw_change_address_script_pubkey =
            self.internal_config().get_change_script_pubkey();
        let original_tx =
            original_tx_btc_pending_info.get_transaction(&self.internal_config().chain);
        let target_address_script_pubkey = original_tx
            .output()
            .iter()
            .find(|v| v.script_pubkey != withdraw_change_address_script_pubkey)
            .cloned()
            .expect("The original tx is not a user withdraw tx.")
            .script_pubkey;

        let target_address = Address::from_script(
            target_address_script_pubkey.as_script(),
            self.internal_config().chain.clone(),
        )
        .expect("Error on extract recipient address from script pubkey")
        .to_string();

        target_address
    }
}
