use near_sdk::{serde_json::Value, PromiseResult};

use crate::{
    burn::GAS_FOR_BURN_CALL,
    env, ext_nbtc,
    mint::{GAS_FOR_MINT_CALL, GAS_FOR_MINT_CALL_BACK},
    near, promise_result_as_success, require, serde_json, AccountId, Contract, ContractExt,
    DepositMsg, Event, Gas, NearToken, PendingUTXOInfo, PostAction, Promise, PromiseOrValue,
    SafeDepositMsg, U128,
};

pub const GAS_FOR_VERIFY_DEPOSIT_CALL_BACK: Gas = Gas::from_tgas(190);
pub const GAS_FOR_UNAVAILABLE_UTXO_CALL_BACK: Gas = Gas::from_tgas(20);

impl Contract {
    pub(crate) fn internal_verify_deposit(
        &mut self,
        deposit_amount: u128,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        pending_utxo_info: PendingUTXOInfo,
        deposit_msg: DepositMsg,
    ) -> Promise {
        let config = self.internal_config();
        let recipient_id = deposit_msg.recipient_id.clone();
        let confirmations = if deposit_msg.extra_msg.is_none() {
            self.get_confirmations(config, deposit_amount)
        } else {
            self.get_extra_msg_confirmations(config, deposit_amount)
        };
        let promise = self.verify_transaction_inclusion_promise(
            config.btc_light_client_account_id.clone(),
            pending_utxo_info.tx_id.clone(),
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            confirmations,
        );

        if deposit_amount < config.min_deposit_amount {
            promise.then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_UNAVAILABLE_UTXO_CALL_BACK)
                    .unavailable_utxo_callback(recipient_id, pending_utxo_info),
            )
        } else {
            let deposit_fee = config.deposit_bridge_fee.get_fee(deposit_amount);
            let mint_amount = deposit_amount - deposit_fee;
            let (protocol_fee, relayer_fee) = config
                .deposit_bridge_fee
                .get_protocol_and_relayer_fee(deposit_fee);

            let post_actions = self.check_deposit_msg(deposit_msg, mint_amount);
            promise.then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_VERIFY_DEPOSIT_CALL_BACK)
                    .verify_deposit_callback(
                        recipient_id,
                        mint_amount.into(),
                        protocol_fee.into(),
                        relayer_fee.into(),
                        pending_utxo_info,
                        post_actions,
                    ),
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn internal_safe_verify_deposit(
        &mut self,
        deposit_amount: u128,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        pending_utxo_info: PendingUTXOInfo,
        recipient_id: AccountId,
        deposit_msg: SafeDepositMsg,
    ) -> Promise {
        let config = self.internal_config();
        let confirmations = self.get_confirmations(config, deposit_amount);
        let promise = self.verify_transaction_inclusion_promise(
            config.btc_light_client_account_id.clone(),
            pending_utxo_info.tx_id.clone(),
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            confirmations,
        );

        if deposit_amount < config.min_deposit_amount {
            promise.then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_UNAVAILABLE_UTXO_CALL_BACK)
                    .unavailable_utxo_callback(recipient_id, pending_utxo_info),
            )
        } else {
            promise.then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_VERIFY_DEPOSIT_CALL_BACK)
                    .verify_safe_deposit_callback(
                        recipient_id,
                        deposit_amount.into(),
                        deposit_msg.msg,
                        pending_utxo_info,
                    ),
            )
        }
    }
}

#[near]
impl Contract {
    #[private]
    pub fn unavailable_utxo_callback(
        &mut self,
        recipient_id: AccountId,
        pending_utxo_info: PendingUTXOInfo,
    ) -> PromiseOrValue<bool> {
        let result_bytes =
            promise_result_as_success().expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");
        require!(
            self.data_mut()
                .verified_deposit_utxo
                .insert(pending_utxo_info.utxo_storage_key.clone()),
            "Already deposit utxo"
        );
        let deposit_amount = u128::from(pending_utxo_info.utxo.balance);
        self.internal_set_unavailable_utxo(
            &pending_utxo_info.utxo_storage_key,
            pending_utxo_info.utxo,
        );
        Event::UnavailableUtxo {
            recipient_id: &recipient_id,
            utxo_storage_key: &pending_utxo_info.utxo_storage_key,
            amount: deposit_amount.into(),
        }
        .emit();
        PromiseOrValue::Value(true)
    }

    #[private]
    pub fn verify_deposit_callback(
        &mut self,
        recipient_id: AccountId,
        mint_amount: U128,
        protocol_fee: U128,
        relayer_fee: U128,
        pending_utxo_info: PendingUTXOInfo,
        post_actions: Option<Vec<PostAction>>,
    ) -> PromiseOrValue<bool> {
        let result_bytes =
            promise_result_as_success().expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");
        require!(
            self.data_mut()
                .verified_deposit_utxo
                .insert(pending_utxo_info.utxo_storage_key.clone()),
            "Already deposit utxo"
        );
        self.internal_mint_promise(
            recipient_id,
            mint_amount,
            protocol_fee,
            relayer_fee,
            pending_utxo_info,
            post_actions,
        )
        .into()
    }

    #[private]
    pub fn verify_safe_deposit_callback(
        &mut self,
        recipient_id: AccountId,
        mint_amount: U128,
        msg: String,
        pending_utxo_info: PendingUTXOInfo,
    ) -> PromiseOrValue<bool> {
        let result_bytes =
            promise_result_as_success().expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");
        require!(
            self.data_mut()
                .verified_deposit_utxo
                .insert(pending_utxo_info.utxo_storage_key.clone()),
            "Already deposit utxo"
        );

        let msg = (!msg.is_empty())
            .then(|| inject_utxo_id_in_msg(msg, &pending_utxo_info.utxo_storage_key));

        ext_nbtc::ext(self.internal_config().nbtc_account_id.clone())
            .with_static_gas(GAS_FOR_MINT_CALL)
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .safe_mint(recipient_id.clone(), mint_amount, msg)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_MINT_CALL_BACK)
                    .safe_mint_callback(recipient_id.clone(), mint_amount, pending_utxo_info),
            )
            .into()
    }

    #[private]
    pub fn safe_mint_callback(
        &mut self,
        recipient_id: AccountId,
        mint_amount: U128,
        pending_utxo_info: PendingUTXOInfo,
    ) -> bool {
        let is_success = !is_refund_required();
        let relayer_account_id = env::signer_account_id();

        if is_success {
            Event::UtxoAdded {
                utxo_storage_keys: vec![pending_utxo_info.utxo_storage_key.clone()],
            }
            .emit();
            self.internal_set_utxo(&pending_utxo_info.utxo_storage_key, pending_utxo_info.utxo);
        } else {
            self.data_mut()
                .verified_deposit_utxo
                .remove(&pending_utxo_info.utxo_storage_key);

            ext_nbtc::ext(self.internal_config().nbtc_account_id.clone())
                .with_static_gas(GAS_FOR_BURN_CALL)
                .burn(
                    env::current_account_id(),
                    mint_amount,
                    relayer_account_id,
                    U128(0),
                );

            Promise::new(env::signer_account_id())
                .transfer(self.required_balance_for_safe_deposit());
        }

        Event::VerifyDepositDetails {
            recipient_id: &recipient_id,
            mint_amount,
            protocol_fee: U128(0),
            relayer_account_id: env::signer_account_id(),
            relayer_fee: U128(0),
            success: is_success,
        }
        .emit();
        is_success
    }
}

fn is_refund_required() -> bool {
    match env::promise_result(0) {
        PromiseResult::Successful(value) => {
            if let Ok(amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                // Normal case: refund if the used token amount is zero
                // The amount can be zero if the `ft_on_transfer` in the receiver contract returns an amount instead of `0`, or if it panics.
                amount.0 == 0
            } else {
                // Unexpected case: don't refund
                false
            }
        }
        // Unexpected case: don't refund
        PromiseResult::Failed => false,
    }
}

fn inject_utxo_id_in_msg(msg: String, utxo_id: &str) -> String {
    if let Ok(mut json) = serde_json::from_str::<Value>(&msg) {
        if let Some(field) = json.get_mut("utxo_id") {
            *field = Value::String(utxo_id.to_string());
            return serde_json::to_string(&json).unwrap();
        }
    }
    msg
}

#[cfg(test)]
mod tests {
    use crate::btc_light_client::deposit::inject_utxo_id_in_msg;
    use near_sdk::{json_types::U128, near, serde_json};

    #[near(serializers=[json])]
    #[derive(Debug, Clone)]
    pub struct UtxoFinTransferMsg {
        pub utxo_id: String,
        pub x: String,
        pub y: U128,
        pub z: String,
    }

    #[test]
    fn test_utxo_id_injection() {
        let duplicated_msg =
            r#"{"utxo_id":"first","utxo_id":"second","x":"some_recipient","y":"1000","z":"OS"}"#
                .to_string();

        println!("Duplicated msg: {}", duplicated_msg);
        let injected_msg = inject_utxo_id_in_msg(duplicated_msg, "correct_utxo_id");
        println!("Injected msg: {}", injected_msg);

        let parsed_msg: UtxoFinTransferMsg = serde_json::from_str(&injected_msg).unwrap();
        assert_eq!(parsed_msg.utxo_id, "correct_utxo_id");

        let expected = r#"{"utxo_id":"correct_utxo_id","x":"some_recipient","y":"1000","z":"OS"}"#;
        assert_eq!(injected_msg, expected);
    }
}
