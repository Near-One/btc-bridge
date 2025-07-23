use crate::*;
use near_plugins::{access_control_any, pause};

#[near]
impl Contract {
    /// Verify that the user has transferred BTC asset to the protocol's designated BTC deposit account, and mint NBTC to the user's NEAR account.
    ///
    /// # Arguments
    ///
    /// * `deposit_msg` - Information used to generate the deposit address path.
    /// * `tx_bytes` - Successfully confirmed BTC transaction bytes
    /// * `vout` - The index of the output where the user sent BTC to the deposit address
    /// * `tx_block_blockhash` - The block hash where the transaction is located.
    /// * `tx_index` - The index of the transaction in the block.
    /// * `merkle_proof` - Merkle proof of the transaction.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC minting was successful.
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_deposit(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        let path = get_deposit_path(&deposit_msg);
        let transaction = bytes_to_btc_transaction(&tx_bytes);
        let deposit_amount = transaction.output()[vout].value.to_sat() as u128;
        require!(deposit_amount > 0, "Invalid deposit_amount");
        require!(
            transaction.lock_time() == LockTime::ZERO,
            "Tx with a non-zero lock_time are not supported."
        );
        let deposit_address = self.generate_utxo_chain_address(&path);
        let deposit_address_script_pubkey = deposit_address.script_pubkey();
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
        let utxo_storage_key = generate_utxo_storage_key(tx_id.clone(), vout as u32);
        self.internal_verify_deposit(
            deposit_amount,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            PendingUTXOInfo {
                tx_id,
                utxo_storage_key,
                utxo,
            },
            deposit_msg,
        )
    }

    /// Verify that the user’s withdrawal has been successful, and then burn the corresponding amount of tokens.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - The transaction ID of the successfully on-chain withdrawal.
    /// * `tx_block_blockhash` - The block hash where the transaction is located.
    /// * `tx_index` - The index of the transaction in the block.
    /// * `merkle_proof` - Merkle proof of the transaction.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC burning was successful.
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_withdraw(
        &mut self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
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
            btc_pending_info,
        )
    }

    /// The user actively increases the gas fee of the Withdraw transaction to accelerate it.
    ///
    /// # Arguments
    ///
    /// * `original_btc_pending_verify_id` - Pending verify ID of the original transaction.
    /// * `output` - Modified output.
    #[pause(except(roles(Role::DAO)))]
    pub fn withdraw_rbf(&mut self, original_btc_pending_verify_id: String, output: Vec<TxOut>) {
        let account_id = env::predecessor_account_id();
        require!(
            self.internal_unwrap_account(&account_id)
                .btc_pending_sign_id
                .is_none(),
            "Previous btc tx has not been signed"
        );
        let btc_pending_id =
            self.internal_withdraw_rbf(&account_id, original_btc_pending_verify_id, output);
        self.internal_unwrap_mut_account(&account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    /// If the user's Withdraw is not verified within a certain time, the protocol can actively cancel the Withdraw through RBF, with the gas fee borne by the user.
    ///
    /// # Arguments
    ///
    /// * `original_btc_pending_verify_id` - Pending verify ID of the original transaction.
    /// * `output` - Modified output.
    #[payable]
    #[access_control_any(roles(Role::DAO, Role::Operator))]
    #[pause(except(roles(Role::DAO)))]
    pub fn cancel_withdraw(&mut self, original_btc_pending_verify_id: String, output: Vec<TxOut>) {
        assert_one_yocto();
        let user_account_id = self
            .internal_unwrap_btc_pending_info(&original_btc_pending_verify_id)
            .account_id
            .clone();
        require!(
            self.internal_unwrap_account(&user_account_id)
                .btc_pending_sign_id
                .is_none(),
            "Assisted user previous btc tx has not been signed"
        );
        let btc_pending_id = self.internal_cancel_withdraw(original_btc_pending_verify_id, output);
        self.internal_unwrap_mut_account(&user_account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &user_account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    /// Verify that the active utxo management has been successful, and then burn the corresponding amount of tokens.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - The transaction ID of the successfully on-chain withdrawal.
    /// * `tx_block_blockhash` - The block hash where the transaction is located.
    /// * `tx_index` - The index of the transaction in the block.
    /// * `merkle_proof` - Merkle proof of the transaction.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC burning was successful.
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_active_utxo_management(
        &mut self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
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
            btc_pending_info,
        )
    }

    /// The number of UTXOs in a Withdraw transaction is managed through outputs that are all change addresses.
    ///
    /// # Arguments
    ///
    /// * `input` - Used to generate the PSBT input.
    /// * `output` -Used to generate the PSBT output.
    #[payable]
    #[access_control_any(roles(Role::DAO, Role::Operator))]
    #[pause(except(roles(Role::DAO)))]
    pub fn active_utxo_management(&mut self, input: Vec<OutPoint>, output: Vec<TxOut>) {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let account = self.internal_unwrap_account(&account_id);
        require!(
            account.btc_pending_sign_id.is_none(),
            "Previous btc tx has not been signed"
        );
        let (psbt, utxo_storage_keys, vutxos) = self.generate_psbt_and_vutxos(input, output);
        let (actual_received_amount, gas_fee) =
            self.check_active_management_psbt_valid(&psbt, &vutxos);
        require!(
            gas_fee <= self.data().cur_available_protocol_fee,
            "Insufficient protocol_fee"
        );
        self.data_mut().cur_available_protocol_fee -= gas_fee;
        self.data_mut().cur_reserved_protocol_fee += gas_fee;

        let need_signature_num = psbt.unsigned_tx.input.len();
        let psbt_hex = psbt.serialize_hex();
        let btc_pending_id = psbt.extract_tx().unwrap().compute_txid().to_string();
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
            #[cfg(feature = "zcash")]
            expiry_height: 0,
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

    /// The initiator of active UTXO management accelerates the transaction by increasing the gas fee.
    ///
    /// # Arguments
    ///
    /// * `original_btc_pending_verify_id` - Pending verify ID of the original transaction.
    /// * `output` - Modified output.
    #[payable]
    #[access_control_any(roles(Role::DAO, Role::Operator))]
    #[pause(except(roles(Role::DAO)))]
    pub fn active_utxo_management_rbf(
        &mut self,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        require!(
            self.internal_unwrap_account(&account_id)
                .btc_pending_sign_id
                .is_none(),
            "Previous btc tx has not been signed"
        );
        let btc_pending_id = self.internal_active_utxo_management_rbf(
            &account_id,
            original_btc_pending_verify_id,
            output,
        );
        self.internal_unwrap_mut_account(&account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    /// Active UTXO management transactions that have not been verified for a long time are allowed to be canceled through RBF.
    ///
    /// # Arguments
    ///
    /// * `original_btc_pending_verify_id` - Pending verify ID of the original transaction.
    /// * `output` - Modified output.
    #[payable]
    #[access_control_any(roles(Role::DAO, Role::Operator))]
    #[pause(except(roles(Role::DAO)))]
    pub fn cancel_active_utxo_management(
        &mut self,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
    ) {
        assert_one_yocto();
        let user_account_id = self
            .internal_unwrap_btc_pending_info(&original_btc_pending_verify_id)
            .account_id
            .clone();
        require!(
            self.internal_unwrap_account(&user_account_id)
                .btc_pending_sign_id
                .is_none(),
            "Assisted user previous btc tx has not been signed"
        );
        let btc_pending_id =
            self.internal_cancel_active_utxo_management(original_btc_pending_verify_id, output);
        self.internal_unwrap_mut_account(&user_account_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::GenerateBtcPendingInfo {
            account_id: &user_account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }

    /// Since there can be many RBFs, removing all RBF pending info at once after verifying the transaction on-chain might not have enough gas.
    /// Therefore, the off-chain program uses this interface to perform the cleanup.
    ///
    /// # Arguments
    ///
    /// * `btc_pending_verify_id` - Invalid pending info ID.
    #[pause(except(roles(Role::DAO)))]
    pub fn clear_invalid_pending_verify_rbf(&mut self, btc_pending_verify_id: String) {
        self.internal_clear_invalid_pending_verify_rbf(btc_pending_verify_id);
    }

    #[pause(except(roles(Role::DAO)))]
    pub fn batch_clear_invalid_pending_verify_rbf(&mut self, btc_pending_verify_ids: Vec<String>) {
        for btc_pending_verify_id in btc_pending_verify_ids {
            self.internal_clear_invalid_pending_verify_rbf(btc_pending_verify_id);
        }
    }

    /// Cancel Withdraw will refund the remaining nBTC to the user. If the refund fails, the user can retrieve it again through this interface.
    #[payable]
    #[pause(except(roles(Role::DAO)))]
    pub fn claim_lost_found(&mut self) -> Promise {
        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let amount = self
            .data_mut()
            .lost_found
            .remove(&account_id)
            .expect("The account does not have lostfound");
        self.internal_transfer_nbtc(&account_id, amount)
    }

    pub fn get_user_deposit_address(&self, deposit_msg: DepositMsg) -> String {
        let path = get_deposit_path(&deposit_msg);
        let deposit_address = self.generate_utxo_chain_address(&path).to_string();
        Event::LogDepositAddress {
            deposit_msg,
            path,
            deposit_address: deposit_address.clone(),
        }
        .emit();
        deposit_address
    }

    pub fn get_change_address(&self) -> Option<String> {
        let config = self.internal_config();
        config.change_address.clone()
    }
}
