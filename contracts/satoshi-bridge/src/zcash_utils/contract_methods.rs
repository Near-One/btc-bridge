use crate::psbt_wrapper::PsbtWrapper;
use crate::*;
use bitcoin::{OutPoint, TxOut};
use near_sdk::json_types::U128;
use near_sdk::{ext_contract, is_promise_success, near, require, AccountId};

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
        orchard_bundle: Option<Vec<u8>>,
        #[callback_unwrap] last_block_height: u32,
    ) -> U128 {
        // If an external Orchard verifier is configured and an Orchard bundle
        // is present, offload proof verification via cross-contract call.
        #[cfg(feature = "zcash")]
        if let (Some(bundle), Some(verifier_id)) = (orchard_bundle.clone(), self.internal_config().orchard_verifier_account_id.clone()) {
            // Compute miner_fee and expected totals on-bridge using selected inputs/outputs.
            let cfg = self.internal_config();
            let change_spk = cfg.get_change_script_pubkey();
            // Sum outputs and enforce they are change-only.
            let mut output_amount: u128 = 0;
            for o in &output {
                require!(o.script_pubkey == change_spk, "Invalid output script_pubkey");
                output_amount += o.value.to_sat() as u128;
            }
            // Sum selected input amounts from storage (do not remove yet).
            let mut input_amount: u128 = 0;
            for op in &input {
                let key = generate_utxo_storage_key(op.txid.to_string(), op.vout);
                let v = self
                    .data()
                    .utxos
                    .get(&key)
                    .unwrap_or_else(|| env::panic_str("UTXO not exist"));
                input_amount += u128::from(v.get_amount());
            }
            let miner_fee = input_amount
                .checked_sub(output_amount)
                .unwrap_or_else(|| env::panic_str("Underflow computing miner fee"));
            require!(
                miner_fee >= cfg.min_btc_gas_fee && miner_fee <= cfg.max_btc_gas_fee,
                format!(
                    "Invalid gas fee ({}). valid range: [{}, {}].",
                    miner_fee, cfg.min_btc_gas_fee, cfg.max_btc_gas_fee
                )
            );
            // Expected total outflow the Orchard side must carry with miner fee included.
            let withdraw_fee = cfg.withdraw_bridge_fee.get_fee(amount.0);
            let expected_total_outflow = amount
                .0
                .checked_sub(withdraw_fee)
                .unwrap_or_else(|| env::panic_str("withdraw fee exceeds amount"));

            // Offload full Orchard verification + policy to the verifier.
            ext_orchard_verifier::ext(verifier_id)
                .with_static_gas(Gas::from_tgas(150))
                .verify_orchard_bundle_with_policy(
                    hex::encode(bundle),
                    target_btc_address.clone(),
                    format!("{:?}", cfg.chain),
                    expected_total_outflow.into(),
                    miner_fee.into(),
                )
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(Gas::from_tgas(120))
                        .orchard_verify_callback(
                            sender_id,
                            amount,
                            target_btc_address,
                            input,
                            output,
                            max_gas_fee,
                            orchard_bundle,
                            last_block_height,
                        ),
                );
            return U128(0);
        }

        // Fallback: perform the rest immediately (no external verify).
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;
        let mut psbt = PsbtWrapper::new(
            input,
            output,
            orchard_bundle,
            expiry_height,
            self.internal_config(),
            Some(target_btc_address.clone()),
            None,
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
    pub fn active_utxo_management_callback(
        &mut self,
        account_id: AccountId,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        #[callback_unwrap] last_block_height: u32,
    ) {
        let expiry_height = last_block_height + self.get_config().expiry_height_gap;

        let mut psbt = PsbtWrapper::new(
            input,
            output,
            None,
            expiry_height,
            self.internal_config(),
            // Active UTXO mgmt uses change-only outputs; no Orchard recipient expected.
            None,
            None,
        );

        self.create_active_utxo_management_pending_info(account_id, &mut psbt);
    }
}

// External interface for the Orchard verifier contract
#[ext_contract(ext_orchard_verifier)]
pub trait OrchardVerifier {
    fn verify_orchard_bundle(&self, bundle_hex: String);
    fn verify_orchard_bundle_with_policy(
        &self,
        bundle_hex: String,
        target_addr: String,
        chain: String,
        expected_total_outflow: near_sdk::json_types::U128,
        miner_fee: near_sdk::json_types::U128,
    );
}

#[near]
impl Contract {
    #[private]
    pub fn orchard_verify_callback(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        orchard_bundle: Option<Vec<u8>>,
        last_block_height: u32,
    ) -> U128 {
        require!(is_promise_success(), "Orchard proof invalid");

        let expiry_height = last_block_height + self.get_config().expiry_height_gap;
        let mut psbt = PsbtWrapper::new(
            input,
            output,
            orchard_bundle,
            expiry_height,
            self.internal_config(),
            Some(target_btc_address.clone()),
            None,
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
        PsbtWrapper::from_original_psbt(
            original_psbt,
            output,
            expiry_height,
            self.internal_config(),
        )
    }
}
