use crate::psbt_wrapper::PsbtWrapper;
use crate::*;
use bitcoin::{OutPoint, TxOut};
use near_sdk::json_types::U128;
use near_sdk::{near, require, AccountId};

pub const GAS_WITHDRAW_CALL_BACK: Gas = Gas::from_tgas(100);
pub const GAS_WITHDRAW_RBF_CALL_BACK: Gas = Gas::from_tgas(100);
pub const GAS_CANCEL_ACTIVA_UTXO_MANAGMENT_CALL_BACK: Gas = Gas::from_tgas(100);
pub const GAS_ACTIVA_UTXO_MANAGMENT_CALL_BACK: Gas = Gas::from_tgas(100);
pub const GAS_FOR_ACTIVE_UTXO_MANAGMENT_CALLBACK: Gas = Gas::from_tgas(100);

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
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let withdraw_rbf_psbt = self.generate_psbt_from_original_psbt_and_new_output(
            original_tx_btc_pending_info,
            output,
            expiry_height,
        );

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

    #[private]
    pub fn cancel_withdraw_callback(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let cancel_withdraw_rbf_psbt = self.generate_psbt_from_original_psbt_and_new_output(
            original_tx_btc_pending_info,
            output,
            expiry_height,
        );

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
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let active_utxo_management_rbf_psbt = self.generate_psbt_from_original_psbt_and_new_output(
            original_tx_btc_pending_info,
            output,
            expiry_height,
        );

        let btc_pending_id = self.internal_active_utxo_management_rbf(
            &account_id,
            original_btc_pending_verify_id,
            active_utxo_management_rbf_psbt,
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
        let original_tx_btc_pending_info =
            self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);
        let cancel_active_utxo_management_rbf_psbt = self
            .generate_psbt_from_original_psbt_and_new_output(
                original_tx_btc_pending_info,
                output,
                expiry_height,
            );

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

    pub(crate) fn ft_on_transfer_withdraw_chain_specific(
        &self,
        sender_id: AccountId,
        amount: u128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) -> PromiseOrValue<U128> {
        PromiseOrValue::Promise(
            self.get_last_block_height_promise().then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_FT_ON_TRANSFER_CALL_BACK)
                    .ft_on_transfer_callback(sender_id, amount, target_btc_address, input, output),
            ),
        )
    }

    pub(crate) fn withdraw_rbf_chain_specific(
        &self,
        account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        self.get_last_block_height_promise().then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_WITHDRAW_RBF_CALL_BACK)
                .withdraw_rbf_callback(account_id, original_btc_pending_verify_id, output),
        );
    }

    pub(crate) fn cancel_withdraw_chain_specific(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        self.get_last_block_height_promise().then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_WITHDRAW_CALL_BACK)
                .cancel_withdraw_callback(user_account_id, original_btc_pending_verify_id, output),
        );
    }

    pub(crate) fn cancel_active_utxo_management_chain_specific(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        self.get_last_block_height_promise().then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_CANCEL_ACTIVA_UTXO_MANAGMENT_CALL_BACK)
                .cancel_active_utxo_managment_callback(
                    user_account_id,
                    original_btc_pending_verify_id,
                    output,
                ),
        );
    }

    pub(crate) fn active_utxo_management_rbf_chain_specific(
        &mut self,
        user_account_id: AccountId,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        self.get_last_block_height_promise().then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_ACTIVA_UTXO_MANAGMENT_CALL_BACK)
                .active_utxo_managment_callback(
                    user_account_id,
                    original_btc_pending_verify_id,
                    output,
                ),
        );
    }

    pub(crate) fn active_utxo_management_chain_specific(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) {
        self.get_last_block_height_promise().then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_ACTIVE_UTXO_MANAGMENT_CALLBACK)
                .active_utxo_management_callback(account_id, input, output),
        );
    }

    pub(crate) fn generate_psbt_from_original_psbt_and_new_output(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        output: Vec<TxOut>,
        expiry_height: u32,
    ) -> PsbtWrapper {
        let original_psbt = original_tx_btc_pending_info.get_psbt();
        PsbtWrapper::from_original_psbt(original_psbt, output, expiry_height)
    }
}
