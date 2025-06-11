use crate::*;

pub const GAS_FOR_BURN_CALL: Gas = Gas::from_tgas(10);
pub const GAS_FOR_WITHDRAW_BURN_CALL_BACK: Gas = Gas::from_tgas(20);
pub const GAS_FOR_ACTIVE_UTXO_MANAGEMENT_BURN_CALL_BACK: Gas = Gas::from_tgas(20);

impl Contract {
    pub fn verify_withdraw_burn_promise(&self, tx_id: String) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        let config = self.internal_config();
        let (protocol_fee, relayer_fee) = config
            .withdraw_bridge_fee
            .get_protocol_and_relayer_fee(btc_pending_info.withdraw_fee);
        ext_nbtc::ext(config.nbtc_account_id.clone())
            .with_static_gas(GAS_FOR_BURN_CALL)
            .burn(
                btc_pending_info.account_id.clone(),
                btc_pending_info.burn_amount.into(),
                env::signer_account_id(),
                relayer_fee.into(),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_WITHDRAW_BURN_CALL_BACK)
                    .verify_withdraw_burn_callback(tx_id, protocol_fee.into(), relayer_fee.into()),
            )
    }

    pub fn verify_active_utxo_management_burn_promise(&self, tx_id: String) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        ext_nbtc::ext(self.internal_config().nbtc_account_id.clone())
            .with_static_gas(GAS_FOR_BURN_CALL)
            .burn(
                btc_pending_info.account_id.clone(),
                btc_pending_info.burn_amount.into(),
                env::signer_account_id(),
                0.into(),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_ACTIVE_UTXO_MANAGEMENT_BURN_CALL_BACK)
                    .verify_active_utxo_management_burn_callback(tx_id),
            )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn verify_withdraw_burn_callback(
        &mut self,
        tx_id: String,
        protocol_fee: U128,
        relayer_fee: U128,
    ) -> bool {
        let is_success = is_promise_success();
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id).clone();
        let config = self.internal_config();
        let refund = if btc_pending_info.is_cancel_withdraw_rbf() {
            btc_pending_info
                .transfer_amount
                .saturating_sub(btc_pending_info.withdraw_fee + btc_pending_info.burn_amount)
        } else {
            0
        };
        let burn_event = Event::VerifyWithdrawDetails {
            account_id: &btc_pending_info.account_id.clone(),
            burn_amount: btc_pending_info.burn_amount.into(),
            protocol_fee,
            relayer_account_id: env::signer_account_id(),
            relayer_fee,
            refund: refund.into(),
            success: is_success,
        };
        if is_success {
            let tx_bytes = btc_pending_info.tx_bytes_with_sign.as_ref().unwrap();
            let transaction = bytes_to_btc_transaction(tx_bytes);
            let withdraw_change_address = config.get_change_address();
            let withdraw_change_script_pubkey = withdraw_change_address.script_pubkey();
            let mut utxo_storage_keys = vec![];
            for (index, output) in transaction.output.into_iter().enumerate() {
                if withdraw_change_script_pubkey == output.script_pubkey {
                    let utxo = UTXO {
                        path: env::current_account_id().to_string(),
                        tx_bytes: tx_bytes.clone(),
                        vout: index,
                        balance: output.value.to_sat(),
                    };
                    let utxo_storage_key = generate_utxo_storage_key(tx_id.clone(), index as u32);
                    self.internal_set_utxo(&utxo_storage_key, utxo);
                    utxo_storage_keys.push(utxo_storage_key);
                }
            }
            if let Some(original_tx_id) = btc_pending_info.get_original_tx_id() {
                self.data_mut().rbf_txs.remove(original_tx_id);
                self.internal_unwrap_mut_account(&btc_pending_info.account_id)
                    .btc_pending_verify_list
                    .remove(original_tx_id);
                let original_tx_btc_pending_info =
                    self.internal_remove_btc_pending_info(original_tx_id);
                if let Some(U128(cancel_rbf_reserved)) =
                    original_tx_btc_pending_info.get_cancel_rbf_reserved()
                {
                    if cancel_rbf_reserved > 0 {
                        self.data_mut().cur_reserved_protocol_fee -= cancel_rbf_reserved;
                        if btc_pending_info.is_cancel_withdraw_rbf() {
                            self.data_mut().acc_protocol_fee_for_gas += cancel_rbf_reserved;
                        } else {
                            self.data_mut().cur_available_protocol_fee += cancel_rbf_reserved;
                        }
                    }
                }
            } else {
                self.internal_unwrap_mut_account(&btc_pending_info.account_id)
                    .btc_pending_verify_list
                    .remove(&tx_id);
                self.data_mut().rbf_txs.remove(&tx_id);

                if let Some(U128(cancel_rbf_reserved)) = btc_pending_info.get_cancel_rbf_reserved()
                {
                    if cancel_rbf_reserved > 0 {
                        self.data_mut().cur_reserved_protocol_fee -= cancel_rbf_reserved;
                        self.data_mut().cur_available_protocol_fee += cancel_rbf_reserved;
                    }
                }
            }
            if protocol_fee.0 > 0 {
                self.data_mut().acc_collected_protocol_fee += protocol_fee.0;
                self.data_mut().cur_available_protocol_fee += protocol_fee.0;
            }
            if refund > 0 {
                self.internal_transfer_nbtc(&btc_pending_info.account_id, refund);
            }
            self.internal_remove_btc_pending_info(&tx_id);
            Event::UtxoAdded { utxo_storage_keys }.emit();
        } else {
            self.internal_unwrap_mut_btc_pending_info(&tx_id)
                .to_pending_verify_stage();
        }
        burn_event.emit();
        is_success
    }

    #[private]
    pub fn verify_active_utxo_management_burn_callback(&mut self, tx_id: String) -> bool {
        let is_success = is_promise_success();
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id).clone();
        let burn_event = Event::VerifyActiveUtxoManagementDetails {
            account_id: &btc_pending_info.account_id.clone(),
            burn_amount: btc_pending_info.burn_amount.into(),
            success: is_success,
        };
        if is_success {
            let tx_bytes = btc_pending_info.tx_bytes_with_sign.as_ref().unwrap();
            let transaction = bytes_to_btc_transaction(tx_bytes);
            let config = self.internal_config();
            let withdraw_change_address = config.get_change_address();
            let withdraw_change_script_pubkey = withdraw_change_address.script_pubkey();
            let mut utxo_storage_keys = vec![];
            for (index, output) in transaction.output.into_iter().enumerate() {
                if withdraw_change_script_pubkey == output.script_pubkey {
                    let utxo = UTXO {
                        path: env::current_account_id().to_string(),
                        tx_bytes: tx_bytes.clone(),
                        vout: index,
                        balance: output.value.to_sat(),
                    };
                    let utxo_storage_key = generate_utxo_storage_key(tx_id.clone(), index as u32);
                    self.internal_set_utxo(&utxo_storage_key, utxo);
                    utxo_storage_keys.push(utxo_storage_key);
                }
            }
            if let Some(original_tx_id) = btc_pending_info.get_original_tx_id() {
                self.data_mut().rbf_txs.remove(original_tx_id);
                self.internal_unwrap_mut_account(&btc_pending_info.account_id)
                    .btc_pending_verify_list
                    .remove(original_tx_id);
                let original_tx_btc_pending_info =
                    self.internal_remove_btc_pending_info(original_tx_id);
                let reserved_protocol_fee = original_tx_btc_pending_info.get_max_gas_fee();
                let unused_reserved_protocol_fee =
                    reserved_protocol_fee - btc_pending_info.burn_amount;
                self.data_mut().cur_reserved_protocol_fee -= reserved_protocol_fee;
                self.data_mut().cur_available_protocol_fee += unused_reserved_protocol_fee;
                self.data_mut().acc_protocol_fee_for_gas += btc_pending_info.burn_amount;
            } else {
                self.internal_unwrap_mut_account(&btc_pending_info.account_id)
                    .btc_pending_verify_list
                    .remove(&tx_id);
                let reserved_protocol_fee = btc_pending_info.get_max_gas_fee();
                let unused_reserved_protocol_fee =
                    reserved_protocol_fee - btc_pending_info.burn_amount;
                self.data_mut().cur_reserved_protocol_fee -= reserved_protocol_fee;
                self.data_mut().cur_available_protocol_fee += unused_reserved_protocol_fee;
                self.data_mut().acc_protocol_fee_for_gas += btc_pending_info.burn_amount;
                self.data_mut().rbf_txs.remove(&tx_id);
            }
            self.internal_remove_btc_pending_info(&tx_id);
            Event::UtxoAdded { utxo_storage_keys }.emit();
        } else {
            self.internal_unwrap_mut_btc_pending_info(&tx_id)
                .to_pending_verify_stage();
        }
        burn_event.emit();
        is_success
    }
}
