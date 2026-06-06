#![allow(clippy::too_many_arguments)]

use crate::psbt_wrapper::PsbtWrapper;
use crate::zcash_utils::types::ChainSpecificData;
use crate::*;
use bitcoin::{OutPoint, TxOut};
use near_sdk::json_types::U128;
use near_sdk::{near, require, AccountId};

pub const GAS_RBF_CALL_BACK: Gas = Gas::from_tgas(100);
pub const GAS_FOR_ACTIVE_UTXO_MANAGMENT_CALLBACK: Gas = Gas::from_tgas(100);

macro_rules! define_rbf_callback {
    ($method:ident, $callback_name:ident, $internal_fn:ident) => {
        impl Contract {
            pub(crate) fn $method(
                &mut self,
                user_account_id: AccountId,
                original_btc_pending_verify_id: String,
                output: Vec<TxOut>,
                chain_specific_data: Option<ChainSpecificData>,
            ) {
                let predecessor_account_id = env::predecessor_account_id();
                let _ = self.get_last_block_height_promise().then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_RBF_CALL_BACK)
                        .$callback_name(
                            user_account_id,
                            original_btc_pending_verify_id,
                            output,
                            chain_specific_data,
                            predecessor_account_id,
                        ),
                );
            }
        }

        #[near]
        impl Contract {
            #[private]
            pub fn $callback_name(
                &mut self,
                account_id: AccountId,
                original_btc_pending_verify_id: String,
                output: Vec<TxOut>,
                chain_specific_data: Option<ChainSpecificData>,
                presecessor_account_id: AccountId,
                #[callback_unwrap] last_block_height: u32,
            ) {
                let expiry_height = self.get_expiry_height(&chain_specific_data, last_block_height);
                let orchard_bundle_bytes = chain_specific_data.map(|c| c.orchard_bundle_bytes);

                let original_tx_btc_pending_info =
                    self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);

                let new_psbt = self.generate_psbt_from_original_psbt_and_new_output(
                    original_tx_btc_pending_info,
                    output,
                    orchard_bundle_bytes.map(|b| b.0),
                    expiry_height,
                    last_block_height,
                );

                let btc_pending_id = self.$internal_fn(
                    &account_id,
                    original_btc_pending_verify_id,
                    new_psbt,
                    presecessor_account_id,
                );

                self.internal_unwrap_mut_account(&account_id)
                    .btc_pending_sign_ids
                    .insert(btc_pending_id.clone());

                Event::GenerateBtcPendingInfo {
                    account_id: &account_id,
                    btc_pending_id: &btc_pending_id,
                }
                .emit();
            }
        }
    };
}

define_rbf_callback!(
    withdraw_rbf_chain_specific,
    withdraw_rbf_callback,
    internal_withdraw_rbf
);
define_rbf_callback!(
    cancel_withdraw_chain_specific,
    cancel_withdraw_callback,
    internal_cancel_withdraw
);
define_rbf_callback!(
    active_utxo_management_rbf_chain_specific,
    active_utxo_management_rbf_callback,
    internal_active_utxo_management_rbf
);
define_rbf_callback!(
    cancel_active_utxo_management_chain_specific,
    cancel_active_utxo_management_callback,
    internal_cancel_active_utxo_management
);

#[allow(clippy::too_many_arguments)]
#[near]
impl Contract {
    #[private]
    pub fn ft_on_transfer_callback(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        chain_specific_data: Option<ChainSpecificData>,
        #[callback_unwrap] last_block_height: u32,
    ) -> U128 {
        let expiry_height = self.get_expiry_height(&chain_specific_data, last_block_height);
        let orchard_bundle = chain_specific_data.map(|c| c.orchard_bundle_bytes.0);

        let psbt = PsbtWrapper::new(
            input,
            output,
            orchard_bundle,
            expiry_height,
            last_block_height,
            Some(target_btc_address.clone()),
            self.internal_config(),
        );

        self.create_btc_pending_info(sender_id, amount.0, target_btc_address, psbt, max_gas_fee);

        U128(0)
    }

    #[private]
    #[allow(clippy::too_many_arguments)]
    pub fn active_utxo_management_callback(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        // For active UTXO management, we don't validate orchard recipient/amount
        // as this is internal bridge operations, not user withdrawals
        let psbt = PsbtWrapper::new(
            input,
            output,
            None,
            expiry_height,
            last_block_height,
            None,
            self.internal_config(),
        );

        self.create_active_utxo_management_pending_info(account_id, psbt);
    }
}

impl Contract {
    fn get_expiry_height(
        &self,
        chain_specific_data: &Option<ChainSpecificData>,
        last_block_height: u32,
    ) -> u32 {
        let expiry_height = if let Some(chain_specific_data) = chain_specific_data {
            chain_specific_data.expiry_height
        } else {
            last_block_height + self.get_config().expiry_height_gap
        };

        require!(
            expiry_height >= last_block_height + self.get_config().expiry_height_gap
                && expiry_height <= last_block_height + 2 * self.get_config().expiry_height_gap,
            format!(
                "Invalid expiry height: {}. Expected value between {} and {}.",
                expiry_height,
                last_block_height + self.get_config().expiry_height_gap,
                last_block_height + 2 * self.get_config().expiry_height_gap
            )
        );

        expiry_height
    }

    pub(crate) fn check_psbt_chain_specific(
        &self,
        psbt: &PsbtWrapper,
        gas_fee: u128,
        target_btc_address: String,
    ) {
        let min_fee = psbt.get_min_fee();
        require!(
            gas_fee >= min_fee.into_u64() as u128,
            format!(
                "Invalid gas fee ({}). min fee = {}.",
                gas_fee,
                min_fee.into_u64()
            )
        );

        // For withdrawals with Orchard bundle, calculate the expected net amount after fees
        if psbt.has_orchard_bundle() {
            psbt.validate_orchard_bundle(target_btc_address, self.internal_config().chain.clone());
        }
    }

    pub(crate) fn check_withdraw_chain_specific(
        _original_tx_btc_pending_info: &BTCPendingInfo,
        _gas_fee: u128,
    ) {
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn ft_on_transfer_withdraw_chain_specific(
        &self,
        sender_id: AccountId,
        amount: u128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        chain_specific_data: Option<ChainSpecificData>,
    ) -> PromiseOrValue<U128> {
        PromiseOrValue::Promise(
            self.get_last_block_height_promise().then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_FT_ON_TRANSFER_CALL_BACK)
                    .ft_on_transfer_callback(
                        sender_id,
                        amount.into(),
                        target_btc_address,
                        input,
                        output,
                        max_gas_fee,
                        chain_specific_data,
                    ),
            ),
        )
    }

    pub(crate) fn active_utxo_management_chain_specific(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) {
        let _ = self.get_last_block_height_promise().then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_ACTIVE_UTXO_MANAGMENT_CALLBACK)
                .active_utxo_management_callback(account_id, input, output),
        );
    }

    pub(crate) fn generate_psbt_from_original_psbt_and_new_output(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        output: Vec<TxOut>,
        orchard_bundle_bytes: Option<Vec<u8>>,
        expiry_height: u32,
        current_height: u32,
    ) -> PsbtWrapper {
        let original_psbt = original_tx_btc_pending_info.get_psbt();
        PsbtWrapper::from_original_psbt(
            original_psbt,
            output,
            orchard_bundle_bytes,
            expiry_height,
            current_height,
            self.internal_config(),
        )
    }

    pub(crate) fn rbf_subsidize_chain_specific(
        &mut self,
        _amount: u128,
        _sender_id: AccountId,
        _pending_tx_id: String,
        _output: Vec<TxOut>,
    ) -> PromiseOrValue<U128> {
        unimplemented!("This function is not supported yet");
    }
}
