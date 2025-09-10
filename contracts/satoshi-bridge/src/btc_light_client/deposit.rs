use crate::*;

pub const GAS_FOR_VERIFY_DEPOSIT_CALL_BACK: Gas = Gas::from_tgas(190);
pub const GAS_FOR_UNAVAILABLE_UTXO_CALL_BACK: Gas = Gas::from_tgas(20);

impl Contract {
    pub fn internal_verify_deposit(
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

            let post_actions = self.check_deposit_msg(deposit_msg, mint_amount, pending_utxo_info.tx_id.clone());
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
        let deposit_amount = pending_utxo_info.utxo.balance as u128;
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
}
