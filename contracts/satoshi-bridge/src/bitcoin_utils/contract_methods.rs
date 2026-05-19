use crate::bitcoin_utils::types::ChainSpecificData;
use crate::env;
use crate::psbt_wrapper::PsbtWrapper;
use crate::{BTCPendingInfo, Contract, Event};
use bitcoin::{OutPoint, TxOut};
use near_sdk::json_types::U128;
use near_sdk::{require, AccountId, PromiseOrValue};

macro_rules! define_rbf_method {
    ($method:ident, $internal_fn:ident) => {
        pub(crate) fn $method(
            &mut self,
            account_id: AccountId,
            original_btc_pending_verify_id: String,
            output: Vec<TxOut>,
            _chain_specific_data: Option<ChainSpecificData>,
        ) -> String {
            let predecessor_account_id = env::predecessor_account_id();
            let original_tx_btc_pending_info =
                self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);

            let new_psbt = self.generate_psbt_from_original_psbt_and_new_output(
                original_tx_btc_pending_info,
                output,
            );

            let btc_pending_id = self.$internal_fn(
                &account_id,
                original_btc_pending_verify_id,
                new_psbt,
                predecessor_account_id,
            );

            self.internal_unwrap_mut_account(&account_id)
                .btc_pending_sign_ids
                .insert(btc_pending_id.clone());

            Event::GenerateBtcPendingInfo {
                account_id: &account_id,
                btc_pending_id: &btc_pending_id,
            }
            .emit();

            btc_pending_id
        }
    };
}

impl Contract {
    pub(crate) fn check_psbt_chain_specific(
        &self,
        _psbt: &PsbtWrapper,
        _gas_fee: u128,
        _target_btc_address: String,
    ) {
    }

    pub(crate) fn check_withdraw_chain_specific(
        original_tx_btc_pending_info: &BTCPendingInfo,
        gas_fee: u128,
    ) {
        // Ensure that the RBF transaction pays more gas than the previous transaction.
        let max_gas_fee = original_tx_btc_pending_info.get_max_gas_fee();
        let additional_gas_amount = gas_fee.saturating_sub(max_gas_fee);
        require!(
            additional_gas_amount > 0,
            format!("No gas increase. Old gas fee = {max_gas_fee}, new gas fee = {gas_fee}")
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn ft_on_transfer_withdraw_chain_specific(
        &mut self,
        sender_id: AccountId,
        amount: u128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        _chain_specific_data: Option<ChainSpecificData>,
    ) -> PromiseOrValue<U128> {
        self.create_btc_pending_info(
            sender_id,
            amount,
            target_btc_address,
            PsbtWrapper::new(input, output),
            max_gas_fee,
        );
        PromiseOrValue::Value(U128(0))
    }

    define_rbf_method!(withdraw_rbf_chain_specific, internal_withdraw_rbf);
    define_rbf_method!(cancel_withdraw_chain_specific, internal_cancel_withdraw);
    define_rbf_method!(
        cancel_active_utxo_management_chain_specific,
        internal_cancel_active_utxo_management
    );
    define_rbf_method!(
        active_utxo_management_rbf_chain_specific,
        internal_active_utxo_management_rbf
    );

    pub(crate) fn active_utxo_management_chain_specific(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) {
        self.create_active_utxo_management_pending_info(
            account_id,
            PsbtWrapper::new(input, output),
        );
    }

    pub(crate) fn generate_psbt_from_original_psbt_and_new_output(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        output: Vec<TxOut>,
    ) -> PsbtWrapper {
        let original_psbt = original_tx_btc_pending_info.get_psbt();
        PsbtWrapper::from_original_psbt(original_psbt, output)
    }

    pub(crate) fn rbf_subsidize_chain_specific(
        &mut self,
        amount: u128,
        sender_id: AccountId,
        pending_tx_id: String,
        output: Vec<TxOut>,
    ) -> PromiseOrValue<U128> {
        let origin_tx_btc_pending_info = self.internal_unwrap_btc_pending_info(&pending_tx_id);
        let user_account_id = origin_tx_btc_pending_info.account_id.clone();
        self.require_pending_sign_capacity(&user_account_id);
        let full_subsidy_amount = self
            .internal_unwrap_btc_pending_info(&pending_tx_id)
            .get_subsidize_amount()
            + amount;
        self.internal_unwrap_mut_btc_pending_info(&pending_tx_id)
            .update_subsidize_amount(full_subsidy_amount);

        let new_pending_info_id = self.withdraw_rbf_chain_specific(
            user_account_id.clone(),
            pending_tx_id.clone(),
            output,
            None,
        );

        let origin_tx_btc_pending_info = self.internal_unwrap_btc_pending_info(&pending_tx_id);
        let new_tx_btc_pending_info = self.internal_unwrap_btc_pending_info(&new_pending_info_id);

        require!(
            new_tx_btc_pending_info.actual_received_amount
                == origin_tx_btc_pending_info.actual_received_amount,
            "Actual received amount has been changed."
        );
        let gas_fee_diff = new_tx_btc_pending_info
            .gas_fee
            .saturating_sub(origin_tx_btc_pending_info.gas_fee);
        require!(
            gas_fee_diff == full_subsidy_amount,
            "Gas fee diff is not equal to subsidy amount."
        );

        Event::SubsidizeRbf {
            origin_btc_pending_id: &pending_tx_id,
            subsidy_amount: U128(amount),
            full_subsidy_amount: U128(full_subsidy_amount),
            subsidizer: &sender_id,
            beneficiary: &user_account_id,
        }
        .emit();

        PromiseOrValue::Value(U128(0))
    }
}
