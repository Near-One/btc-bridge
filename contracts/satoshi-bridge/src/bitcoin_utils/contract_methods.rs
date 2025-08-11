use crate::psbt_wrapper::PsbtWrapper;
use crate::{Contract, Event, VUTXO};
use bitcoin::{OutPoint, TxOut};
use near_sdk::{env, AccountId, PromiseOrValue};

impl Contract {
    pub(crate) fn check_psbt_chain_specific(
        &self,
        psbt: &PsbtWrapper,
        vutxos: &[VUTXO],
        gas_fee: u128,
    ) {
    }

    pub(crate) fn ft_on_transfer_withdraw_chain_specific(
        &self,
        sender_id: AccountId,
        amount: u128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) -> PromiseOrValue<U128> {
        let mut psbt = PsbtWrapper::new(input, output);
        self.create_btc_pending_info(sender_id, amount, target_btc_address, psbt);
        PromiseOrValue::Value(U128(0))
    }

    pub(crate) fn withdraw_rbf_chain_specific(
        &self,
        account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) -> PromiseOrValue<U128> {
        let btc_pending_id =
            self.internal_withdraw_rbf(&account_id, original_btc_pending_verify_id, output);
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
        let btc_pending_id = self.internal_cancel_withdraw(original_btc_pending_verify_id, output);
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
        let btc_pending_id =
            self.internal_cancel_active_utxo_management(original_btc_pending_verify_id, output);
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
        let btc_pending_id = self.internal_active_utxo_management_rbf(
            &user_account_id,
            original_btc_pending_verify_id,
            output,
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
        create_active_utxo_management_pending_info(account_id, &mut psbt);
    }
}
