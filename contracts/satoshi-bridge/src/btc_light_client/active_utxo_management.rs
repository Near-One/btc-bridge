use crate::{
    env, near, psbt_wrapper::PsbtWrapper, require, serde_json, utils::nano_to_sec, AccountId,
    BTCPendingInfo, Contract, ContractExt, Event, Gas, OriginalState, PendingInfoStage,
    PendingInfoState, Promise, PromiseOrValue, MAX_BOOL_RESULT,
};

pub const GAS_FOR_VERIFY_ACTIVE_UTXO_MANAGEMENT_CALL_BACK: Gas = Gas::from_tgas(50);

impl Contract {
    pub fn internal_verify_active_utxo_management(
        &self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
        btc_pending_info: &BTCPendingInfo,
    ) -> Promise {
        let config = self.internal_config();
        let confirmations = self.get_confirmations(config, btc_pending_info.actual_received_amount);
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
                .with_static_gas(GAS_FOR_VERIFY_ACTIVE_UTXO_MANAGEMENT_CALL_BACK)
                .verify_active_utxo_management_callback(tx_id),
        )
    }

    pub(crate) fn internal_verify_active_utxo_management_entry(
        &mut self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
    ) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        btc_pending_info.assert_active_utxo_management_related_pending_verify_tx();
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
        self.internal_verify_active_utxo_management(
            tx_id,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            coinbase_proof,
            btc_pending_info,
        )
    }

    pub fn create_active_utxo_management_pending_info(
        &mut self,
        account_id: AccountId,
        mut psbt: PsbtWrapper,
    ) {
        let account = self.internal_unwrap_account(&account_id);
        require!(
            account.btc_pending_sign_id.is_none(),
            "Previous btc tx has not been signed"
        );

        let (utxo_storage_keys, vutxos) = self.generate_vutxos(&mut psbt);
        let (actual_received_amount, gas_fee) =
            self.check_active_management_psbt_valid(&psbt, &vutxos);
        require!(
            gas_fee <= self.data().cur_available_protocol_fee,
            "Insufficient protocol_fee"
        );
        self.data_mut().cur_available_protocol_fee -= gas_fee;
        self.data_mut().cur_reserved_protocol_fee += gas_fee;

        let need_signature_num = psbt.get_input_num();
        let psbt_hex = psbt.serialize();
        let btc_pending_id = psbt.get_pending_id();
        let btc_pending_info = BTCPendingInfo {
            account_id: account_id.clone(),
            btc_pending_id: btc_pending_id.clone(),
            transfer_amount: 0,
            actual_received_amount,
            withdraw_fee: 0,
            gas_fee,
            burn_amount: gas_fee,
            psbt_hex,
            vutxos,
            signatures: vec![None; need_signature_num],
            tx_bytes_with_sign: None,
            create_time_sec: nano_to_sec(env::block_timestamp()),
            last_sign_time_sec: 0,
            state: PendingInfoState::ActiveUtxoManagementOriginal(OriginalState {
                stage: PendingInfoStage::PendingSign,
                max_gas_fee: gas_fee,
                last_rbf_time_sec: None,
                cancel_rbf_reserved: None,
            }),
        };
        require!(
            self.data_mut()
                .btc_pending_infos
                .insert(btc_pending_id.clone(), btc_pending_info.into())
                .is_none(),
            "pending info already exist"
        );
        self.internal_unwrap_mut_account(&account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::UtxoRemoved { utxo_storage_keys }.emit();
        Event::GenerateBtcPendingInfo {
            account_id: &account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }
}

#[near]
impl Contract {
    #[private]
    pub fn verify_active_utxo_management_callback(
        &mut self,
        tx_id: String,
    ) -> PromiseOrValue<bool> {
        let result_bytes = env::promise_result_checked(0, MAX_BOOL_RESULT)
            .expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");
        self.internal_unwrap_btc_pending_info(&tx_id)
            .assert_pending_verify();
        self.internal_unwrap_mut_btc_pending_info(&tx_id)
            .to_pending_burn_stage();
        self.verify_active_utxo_management_burn_promise(tx_id)
            .into()
    }
}
