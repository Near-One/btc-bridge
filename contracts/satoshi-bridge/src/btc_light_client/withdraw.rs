use crate::{
    env, near, require, serde_json, BTCPendingInfo, Contract, ContractExt, Gas, Promise,
    PromiseOrValue, MAX_BOOL_RESULT,
};

pub const GAS_FOR_VERIFY_WITHDRAW_CALL_BACK: Gas = Gas::from_tgas(50);
pub const GAS_FOR_VERIFY_CANCEL_WITHDRAW_CALL_BACK: Gas = Gas::from_tgas(50);

impl Contract {
    pub fn internal_verify_withdraw(
        &self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
        btc_pending_info: &BTCPendingInfo,
    ) -> Promise {
        let config = self.internal_config();
        let confirmations = config.get_confirmations(btc_pending_info.actual_received_amount)
            + self.relayer_delta_for_predecessor();
        self.verify_transaction_inclusion_promise(
            config.btc_light_client_account_id.clone(),
            tx_id.clone(),
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            coinbase_proof,
            confirmations,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_VERIFY_CANCEL_WITHDRAW_CALL_BACK)
                .internal_verify_withdraw_callback(tx_id),
        )
    }

    pub(crate) fn internal_verify_withdraw_entry(
        &mut self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
    ) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        btc_pending_info.assert_withdraw_related_pending_verify_tx();
        if let Some(original_tx_id) = btc_pending_info.get_original_tx_id() {
            require!(
                self.check_btc_pending_info_exists(original_tx_id),
                "original tx already verified"
            );
        }
        require!(
            btc_pending_info.tx_bytes_with_sign.is_some(),
            "Missing tx_bytes_with_sign"
        );
        self.internal_verify_withdraw(
            tx_id,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            coinbase_proof,
            btc_pending_info,
        )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn internal_verify_withdraw_callback(&mut self, tx_id: String) -> PromiseOrValue<bool> {
        let result_bytes = env::promise_result_checked(0, MAX_BOOL_RESULT)
            .expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");
        self.internal_unwrap_btc_pending_info(&tx_id)
            .assert_pending_verify();
        self.internal_unwrap_mut_btc_pending_info(&tx_id)
            .to_pending_burn_stage();
        self.verify_withdraw_burn_promise(tx_id).into()
    }
}
