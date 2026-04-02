use crate::dash_utils::types::ChainSpecificData;
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
        ) {
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
                .btc_pending_sign_id = Some(btc_pending_id.clone());

            Event::GenerateBtcPendingInfo {
                account_id: &account_id,
                btc_pending_id: &btc_pending_id,
            }
            .emit();
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
        require!(additional_gas_amount > 0, "No gas increase.");
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
}
