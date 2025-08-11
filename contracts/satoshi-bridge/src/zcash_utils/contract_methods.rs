use crate::psbt_wrapper::PsbtWrapper;
use crate::*;
use bitcoin::{OutPoint, TxOut};
use near_sdk::json_types::U128;
use near_sdk::{near, require, AccountId};

#[near]
impl Contract {
    #[private]
    pub fn ft_on_transfer_callback(
        &mut self,
        sender_id: AccountId,
        amount: u128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) -> U128 {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;
        let mut psbt = PsbtWrapper::new(input, output, expiry_height);
        self.create_btc_pending_info(sender_id, amount, target_btc_address, &mut psbt);

        U128(0)
    }

    #[private]
    pub fn withdraw_rbf_callback(
        &mut self,
        account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        let btc_pending_id = self.internal_withdraw_rbf(
            &account_id,
            original_btc_pending_verify_id,
            output,
            expiry_height,
        );
        self.internal_unwrap_mut_account(&account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    #[private]
    pub fn cancel_withdraw_callback(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        let btc_pending_id =
            self.internal_cancel_withdraw(original_btc_pending_verify_id, output, expiry_height);
        self.internal_unwrap_mut_account(&user_account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &user_account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    #[private]
    pub fn active_utxo_management_callback(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        let mut psbt = PsbtWrapper::new(input, output, expiry_height);

        self.create_active_utxo_management_pending_info(account_id, &mut psbt);
    }

    #[private]
    pub fn active_utxo_managment_callback(
        &mut self,
        account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        let btc_pending_id = self.internal_active_utxo_management_rbf(
            &account_id,
            original_btc_pending_verify_id,
            output,
            expiry_height,
        );
        self.internal_unwrap_mut_account(&account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    #[private]
    pub fn cancel_active_utxo_managment_callback(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;
        let btc_pending_id = self.internal_cancel_active_utxo_management(
            original_btc_pending_verify_id,
            output,
            expiry_height,
        );
        self.internal_unwrap_mut_account(&user_account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &user_account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }
}

impl Contract {
    pub(crate) fn check_psbt_chain_specific(
        &self,
        psbt: &PsbtWrapper,
        vutxos: &[VUTXO],
        gas_fee: u128,
    ) {
        let public_key = self.generate_btc_public_key(&vutxos[0].get_path());
        let min_fee = psbt.get_min_fee(&public_key);
        require!(
            gas_fee >= min_fee.into_u64() as u128,
            format!(
                "Invalid gas fee ({}). min fee = {}.",
                gas_fee,
                min_fee.into_u64()
            )
        );
    }
}
