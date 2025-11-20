use crate::psbt_wrapper::PsbtWrapper;
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
            ) {
                self.get_last_block_height_promise().then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(GAS_RBF_CALL_BACK)
                        .$callback_name(user_account_id, original_btc_pending_verify_id, output),
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
                #[callback_unwrap] last_block_height: u32,
            ) {
                let expiry_height = last_block_height + self.get_config().expiry_height_gap;

                let original_tx_btc_pending_info =
                    self.internal_unwrap_btc_pending_info(&original_btc_pending_verify_id);

                let new_psbt = self.generate_psbt_from_original_psbt_and_new_output(
                    original_tx_btc_pending_info,
                    output,
                    expiry_height,
                );

                let btc_pending_id =
                    self.$internal_fn(&account_id, original_btc_pending_verify_id, new_psbt);

                self.internal_unwrap_mut_account(&account_id)
                    .btc_pending_sign_id = Some(btc_pending_id.clone());

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
    #[allow(clippy::too_many_arguments)]
    pub fn ft_on_transfer_callback(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        orchard_bundle: Option<Vec<u8>>,
        #[callback_unwrap] last_block_height: u32,
    ) -> U128 {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        // For withdrawals with Orchard bundle, calculate the expected net amount after fees
        let (expected_recipient, expected_amount) = if orchard_bundle.is_some() {
            let withdraw_fee = self.internal_config().withdraw_bridge_fee.get_fee(amount.0);
            // The orchard amount is the withdrawal amount minus the withdraw fee and gas fee
            // max_gas_fee defaults to 10000 satoshis if not provided
            let gas_fee = max_gas_fee.map(|g| g.0).unwrap_or(10000);
            let orchard_amount = amount
                .0
                .saturating_sub(withdraw_fee)
                .saturating_sub(gas_fee);
            (Some(target_btc_address.clone()), Some(orchard_amount))
        } else {
            (None, None)
        };

        let mut psbt = PsbtWrapper::new(
            input,
            output,
            orchard_bundle,
            expiry_height,
            self.internal_config(),
            expected_recipient,
            expected_amount,
        );
        self.create_btc_pending_info(
            sender_id,
            amount.0,
            target_btc_address,
            &mut psbt,
            max_gas_fee,
        );

        U128(0)
    }

    #[private]
    #[allow(clippy::too_many_arguments)]
    pub fn active_utxo_management_callback(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        orchard_bundle: Option<Vec<u8>>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        // For active UTXO management, we don't validate orchard recipient/amount
        // as this is internal bridge operations, not user withdrawals
        let mut psbt = PsbtWrapper::new(
            input,
            output,
            orchard_bundle,
            expiry_height,
            self.internal_config(),
            None,
            None,
        );

        self.create_active_utxo_management_pending_info(account_id, &mut psbt);
    }
}

impl Contract {
    pub(crate) fn check_psbt_chain_specific(&self, psbt: &PsbtWrapper, gas_fee: u128) {
        let min_fee = psbt.get_min_fee();
        require!(
            gas_fee >= min_fee.into_u64() as u128,
            format!(
                "Invalid gas fee ({}). min fee = {}.",
                gas_fee,
                min_fee.into_u64()
            )
        );
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
        orchard_bundle: Option<Vec<u8>>,
    ) -> PromiseOrValue<U128> {
        // Validate: Unified Address requires Orchard bundle
        if target_btc_address.starts_with("u1") || target_btc_address.starts_with("u") {
            require!(
                orchard_bundle.is_some(),
                "Unified Address provided without Orchard bundle. \
                 Either provide both or use a transparent Zcash address (starts with t1/t3)"
            );
        }

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
                        orchard_bundle,
                    ),
            ),
        )
    }

    pub(crate) fn active_utxo_management_chain_specific(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        orchard_bundle: Option<Vec<u8>>,
    ) {
        self.get_last_block_height_promise().then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_ACTIVE_UTXO_MANAGMENT_CALLBACK)
                .active_utxo_management_callback(account_id, input, output, orchard_bundle),
        );
    }

    pub(crate) fn generate_psbt_from_original_psbt_and_new_output(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        output: Vec<TxOut>,
        expiry_height: u32,
    ) -> PsbtWrapper {
        let original_psbt = original_tx_btc_pending_info.get_psbt();
        PsbtWrapper::from_original_psbt(
            original_psbt,
            output,
            expiry_height,
            self.internal_config(),
        )
    }
}
