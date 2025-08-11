use crate::psbt_wrapper::PsbtWrapper;
use crate::{BTCPendingInfo, Contract, Event, VUTXO};
use bitcoin::{OutPoint, TxOut};
use near_sdk::json_types::U128;
use near_sdk::{env, require, AccountId, PromiseOrValue};

impl Contract {
    pub(crate) fn check_psbt_chain_specific(
        &self,
        psbt: &PsbtWrapper,
        vutxos: &[VUTXO],
        gas_fee: u128,
    ) {
    }

    pub(crate) fn check_withdraw_chain_specific(
        original_tx_btc_pending_info: &BTCPendingInfo,
        gas_fee: u128,
    ) {
        // Ensure that the RBF transaction pays more gas than the previous transaction.
        let max_gas_fee = original_tx_btc_pending_info.get_max_gas_fee();
        let additional_gas_amount = gas_fee.saturating_sub(max_gas_fee);
        require!(additional_gas_amount > 0, "No gas increase.");
    }

    pub(crate) fn ft_on_transfer_withdraw_chain_specific(
        &mut self,
        sender_id: AccountId,
        amount: u128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) -> PromiseOrValue<U128> {
        let mut psbt = PsbtWrapper::new(input, output);
        self.create_btc_pending_info(sender_id, amount, target_btc_address, &mut psbt);
        PromiseOrValue::Value(U128(0))
    }

    pub(crate) fn withdraw_rbf_chain_specific(
        &mut self,
        account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let withdraw_rbf_psbt = self
            .generate_psbt_from_original_psbt_and_new_output(original_tx_btc_pending_info, output);

        let btc_pending_id = self.internal_withdraw_rbf(
            &account_id,
            original_btc_pending_verify_id,
            withdraw_rbf_psbt,
        );
        self.internal_unwrap_mut_account(&account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    pub(crate) fn cancel_withdraw_chain_specific(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let cancel_withdraw_rbf_psbt = self
            .generate_psbt_from_original_psbt_and_new_output(original_tx_btc_pending_info, output);

        let btc_pending_id =
            self.internal_cancel_withdraw(original_btc_pending_verify_id, cancel_withdraw_rbf_psbt);
        self.internal_unwrap_mut_account(&user_account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &user_account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    pub(crate) fn cancel_active_utxo_management_chain_specific(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let cancel_active_utxo_management_rbf_psbt = self
            .generate_psbt_from_original_psbt_and_new_output(original_tx_btc_pending_info, output);

        let btc_pending_id = self.internal_cancel_active_utxo_management(
            original_btc_pending_verify_id,
            cancel_active_utxo_management_rbf_psbt,
        );
        self.internal_unwrap_mut_account(&user_account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &user_account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    pub(crate) fn active_utxo_management_rbf_chain_specific(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let active_utxo_management_rbf_psbt = self
            .generate_psbt_from_original_psbt_and_new_output(original_tx_btc_pending_info, output);

        let btc_pending_id = self.internal_active_utxo_management_rbf(
            &user_account_id,
            original_btc_pending_verify_id,
            active_utxo_management_rbf_psbt,
        );
        self.internal_unwrap_mut_account(&user_account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &user_account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    pub(crate) fn active_utxo_management_chain_specific(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) {
        let mut psbt = PsbtWrapper::new(input, output);
        self.create_active_utxo_management_pending_info(account_id, &mut psbt);
    }

    pub(crate) fn generate_psbt_from_original_psbt_and_new_output(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        output: Vec<TxOut>,
    ) -> PsbtWrapper {
        let original_psbt = original_tx_btc_pending_info.get_psbt();
        PsbtWrapper::from_original_psbt(original_psbt, output)
    }
}
