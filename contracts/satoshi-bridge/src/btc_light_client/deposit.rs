// `verify_deposit_callback` takes 8 args (extra `confirmations_delta`); the `#[near]`
// proc-macro re-emits the signature in the ext-trait so the `clippy::too_many_arguments`
// lint fires from inside the macro expansion and an inner `#[allow]` doesn't reach it.
#![allow(clippy::too_many_arguments)]

use near_sdk::serde_json::Value;

use crate::{
    btc_light_client::TxInclusionInfo,
    burn::GAS_FOR_BURN_CALL,
    deposit_msg::get_deposit_path,
    env, ext_nbtc, generate_utxo_storage_key,
    mint::{GAS_FOR_MINT_CALL, GAS_FOR_MINT_CALL_BACK},
    near, require, serde_json, AccountId, Contract, ContractExt, DepositMsg, Event, Gas, NearToken,
    PendingUTXOInfo, PostAction, Promise, PromiseOrValue, SafeDepositMsg, WrappedTransaction,
    MAX_FT_TRANSFER_CALL_RESULT, MAX_INCLUSION_INFO_RESULT, U128, UTXO,
};

pub const GAS_FOR_VERIFY_DEPOSIT_CALL_BACK: Gas = Gas::from_tgas(130);
pub const GAS_FOR_UNAVAILABLE_UTXO_CALL_BACK: Gas = Gas::from_tgas(20);

impl Contract {
    pub(crate) fn internal_verify_deposit(
        &mut self,
        deposit_amount: u128,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
        pending_utxo_info: PendingUTXOInfo,
        deposit_msg: DepositMsg,
    ) -> Promise {
        let recipient_id = deposit_msg.recipient_id.clone();
        // Predecessor is the original relayer/user here — capture the whitelist
        // delta before the cross-contract call. The callback runs with the contract
        // itself as predecessor and cannot redo this check.
        let confirmations_delta = if deposit_msg.extra_msg.is_none() {
            self.relayer_delta_for_predecessor()
        } else {
            self.extra_msg_relayer_delta_for_predecessor()
        };
        let config = self.internal_config();
        let promise = self.verify_transaction_inclusion_with_heights_promise(
            config.btc_light_client_account_id.clone(),
            pending_utxo_info.tx_id.clone(),
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            coinbase_proof,
        );

        if deposit_amount < config.min_deposit_amount {
            promise.then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_UNAVAILABLE_UTXO_CALL_BACK)
                    .unavailable_utxo_callback(
                        recipient_id,
                        pending_utxo_info,
                        confirmations_delta,
                    ),
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
                        confirmations_delta,
                    ),
            )
        }
    }

    pub(crate) fn internal_safe_verify_deposit(
        &mut self,
        deposit_amount: u128,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
        pending_utxo_info: PendingUTXOInfo,
        recipient_id: AccountId,
        deposit_msg: SafeDepositMsg,
    ) -> Promise {
        let confirmations_delta = self.relayer_delta_for_predecessor();
        let config = self.internal_config();
        let promise = self.verify_transaction_inclusion_with_heights_promise(
            config.btc_light_client_account_id.clone(),
            pending_utxo_info.tx_id.clone(),
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            coinbase_proof,
        );

        if deposit_amount < config.min_deposit_amount {
            promise.then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_UNAVAILABLE_UTXO_CALL_BACK)
                    .unavailable_utxo_callback(
                        recipient_id,
                        pending_utxo_info,
                        confirmations_delta,
                    ),
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
                        confirmations_delta,
                    ),
            )
        }
    }

    /// Parse the LC's `Option<TxInclusionInfo>` response, bump the block-amount
    /// ring with this tx's amount, and panic if depth doesn't satisfy the
    /// confirmations tier for the resulting cumulative.
    ///
    /// Single helper so the three deposit-related callbacks share identical
    /// inclusion-check semantics.
    fn process_inclusion_and_check(
        &mut self,
        pending_utxo_info: &PendingUTXOInfo,
        confirmations_delta: u64,
    ) {
        let result_bytes = env::promise_result_checked(0, MAX_INCLUSION_INFO_RESULT)
            .expect("Call verify_transaction_inclusion_with_heights failed");
        let info: Option<TxInclusionInfo> = serde_json::from_slice(&result_bytes)
            .expect("verify_transaction_inclusion_with_heights returned an unexpected payload");
        let info = info.expect("Transaction not included in the BTC mainchain");
        self.bump_and_check_confirmations(
            info.tx_block_height,
            info.mainchain_tip_height,
            u128::from(pending_utxo_info.utxo.balance),
            confirmations_delta,
        );
    }

    pub(crate) fn internal_verify_deposit_entry(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
    ) -> Promise {
        require!(
            deposit_msg.safe_deposit.is_none(),
            "safe_deposit not supported in verify_deposit"
        );
        let path = get_deposit_path(&deposit_msg);
        let transaction = WrappedTransaction::decode(&tx_bytes, &self.internal_config().chain)
            .expect("Deserialization tx_bytes failed");
        let deposit_amount = u128::from(transaction.output()[vout].value.to_sat());
        require!(deposit_amount > 0, "Invalid deposit_amount");
        let deposit_address = self.generate_utxo_chain_address(&path);
        let deposit_address_script_pubkey = deposit_address
            .script_pubkey()
            .expect("Invalid deposit address");
        require!(
            deposit_address_script_pubkey == transaction.output()[vout].script_pubkey,
            "Invalid deposit tx_bytes"
        );

        let utxo = UTXO {
            path,
            tx_bytes,
            vout,
            balance: transaction.output()[vout].value.to_sat(),
        };
        let tx_id = transaction.compute_txid().to_string();
        let utxo_storage_key = generate_utxo_storage_key(
            tx_id.clone(),
            u32::try_from(vout).unwrap_or_else(|_| env::panic_str("vout overflow")),
        );
        self.internal_verify_deposit(
            deposit_amount,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            coinbase_proof,
            PendingUTXOInfo {
                tx_id,
                utxo_storage_key,
                utxo,
            },
            deposit_msg,
        )
    }

    pub(crate) fn internal_safe_verify_deposit_entry(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
    ) -> Promise {
        require!(
            env::attached_deposit() >= self.required_balance_for_safe_deposit(),
            "Insufficient deposit for storage"
        );

        let path = get_deposit_path(&deposit_msg);
        let safe_deposit_msg = deposit_msg
            .safe_deposit
            .unwrap_or_else(|| env::panic_str("safe_deposit is required in safe_verify_deposit"));

        let transaction = WrappedTransaction::decode(&tx_bytes, &self.internal_config().chain)
            .expect("Deserialization tx_bytes failed");
        let deposit_amount = transaction.output()[vout].value.to_sat().into();
        require!(deposit_amount > 0, "Invalid deposit_amount");
        let deposit_address = self.generate_utxo_chain_address(&path);
        let deposit_address_script_pubkey = deposit_address
            .script_pubkey()
            .expect("Invalid deposit address");
        require!(
            deposit_address_script_pubkey == transaction.output()[vout].script_pubkey,
            "Invalid deposit tx_bytes"
        );

        let tx_bytes = if tx_bytes.len() > 10000 {
            env::log_str("tx_bytes length exceeds 10000, truncating to 300 bytes");
            vec![0u8; 300]
        } else {
            tx_bytes
        };

        let utxo = UTXO {
            path,
            tx_bytes,
            vout,
            balance: transaction.output()[vout].value.to_sat(),
        };
        let tx_id = transaction.compute_txid().to_string();
        let utxo_storage_key = generate_utxo_storage_key(
            tx_id.clone(),
            u32::try_from(vout).unwrap_or_else(|_| env::panic_str("vout overflow")),
        );

        self.internal_safe_verify_deposit(
            deposit_amount,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            coinbase_proof,
            PendingUTXOInfo {
                tx_id,
                utxo_storage_key,
                utxo,
            },
            deposit_msg.recipient_id,
            safe_deposit_msg,
        )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn unavailable_utxo_callback(
        &mut self,
        recipient_id: AccountId,
        pending_utxo_info: PendingUTXOInfo,
        confirmations_delta: u64,
    ) -> PromiseOrValue<bool> {
        self.process_inclusion_and_check(&pending_utxo_info, confirmations_delta);
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
        confirmations_delta: u64,
    ) -> PromiseOrValue<bool> {
        self.process_inclusion_and_check(&pending_utxo_info, confirmations_delta);
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
        confirmations_delta: u64,
    ) -> PromiseOrValue<bool> {
        self.process_inclusion_and_check(&pending_utxo_info, confirmations_delta);
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
                balances: Some(vec![U128(pending_utxo_info.utxo.balance.into())]),
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
                )
                .detach();

            Promise::new(env::signer_account_id())
                .transfer(self.required_balance_for_safe_deposit())
                .detach();
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

/// Refund only if `safe_mint` returned 0. Any other outcome (non-zero
/// amount, unparseable payload, panic) is treated as "UTXO spent, no
/// refund" — for safety, to avoid a potential double spend.
fn is_refund_required() -> bool {
    match env::promise_result_checked(0, MAX_FT_TRANSFER_CALL_RESULT) {
        Ok(value) => {
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
        Err(_) => false,
    }
}

fn inject_utxo_id_in_msg(msg: String, utxo_id: &str) -> String {
    fn inject(value: &mut Value, utxo_id: &str) {
        match value {
            Value::Object(map) => {
                for (k, v) in map.iter_mut() {
                    if k == "utxo_id" {
                        *v = Value::String(utxo_id.to_string());
                    } else {
                        inject(v, utxo_id);
                    }
                }
            }
            Value::Array(arr) => {
                for v in arr.iter_mut() {
                    inject(v, utxo_id);
                }
            }
            _ => {}
        }
    }

    if let Ok(mut json) = serde_json::from_str::<Value>(&msg) {
        inject(&mut json, utxo_id);
        serde_json::to_string(&json).unwrap()
    } else {
        msg
    }
}

#[cfg(test)]
mod tests {
    use crate::btc_light_client::deposit::inject_utxo_id_in_msg;
    use near_sdk::{near, serde_json};

    #[near(serializers=[json])]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UtxoFinTransferMsg {
        pub utxo_id: String,
        pub recipient: String,
        pub relayer_fee: String,
        pub msg: String,
    }

    #[near(serializers=[json])]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum BridgeOnTransferMsg {
        UtxoFinTransfer(UtxoFinTransferMsg),
    }

    #[test]
    fn test_duplicated_utxo_id_injection() {
        let duplicated_msg =
            r#"{"utxo_id":"first","utxo_id":"second","recipient":"some_recipient","relayer_fee":"1000","msg":"OS"}"#
                .to_string();

        let injected_msg = inject_utxo_id_in_msg(duplicated_msg, "correct_utxo_id");
        let parsed_msg: UtxoFinTransferMsg = serde_json::from_str(&injected_msg).unwrap();
        let expected = UtxoFinTransferMsg {
            utxo_id: "correct_utxo_id".to_string(),
            recipient: "some_recipient".to_string(),
            relayer_fee: "1000".to_string(),
            msg: "OS".to_string(),
        };

        assert_eq!(parsed_msg, expected);
    }

    #[test]
    fn test_utxo_id_injection() {
        let nested_msg =
            r#"{"UtxoFinTransfer":{"msg":"OS","recipient":"some_recipient","relayer_fee":"1000","utxo_id":"{{UTXO_TX_ID}}"}}"#
                .to_string();

        let injected_msg = inject_utxo_id_in_msg(nested_msg, "correct_utxo_id");
        let parsed_msg: BridgeOnTransferMsg = serde_json::from_str(&injected_msg).unwrap();
        let expected = BridgeOnTransferMsg::UtxoFinTransfer(UtxoFinTransferMsg {
            utxo_id: "correct_utxo_id".to_string(),
            recipient: "some_recipient".to_string(),
            relayer_fee: "1000".to_string(),
            msg: "OS".to_string(),
        });

        assert_eq!(parsed_msg, expected);
    }

    #[test]
    fn test_already_set_utxo_id_injection() {
        let nested_msg =
            r#"{"UtxoFinTransfer":{"msg":"OS","recipient":"{{UTXO_TX_ID}}","relayer_fee":"1000","utxo_id":"invalid_utxo_id"}}"#
                .to_string();

        let injected_msg = inject_utxo_id_in_msg(nested_msg, "correct_utxo_id");
        let parsed_msg: BridgeOnTransferMsg = serde_json::from_str(&injected_msg).unwrap();
        let expected = BridgeOnTransferMsg::UtxoFinTransfer(UtxoFinTransferMsg {
            utxo_id: "correct_utxo_id".to_string(),
            recipient: "{{UTXO_TX_ID}}".to_string(),
            relayer_fee: "1000".to_string(),
            msg: "OS".to_string(),
        });

        assert_eq!(parsed_msg, expected);
    }
}
