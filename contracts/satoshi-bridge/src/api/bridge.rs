use crate::psbt_wrapper::PsbtWrapper;
use crate::*;
use near_plugins::{access_control_any, pause};
use near_sdk::json_types::Base64VecU8;

#[trusted_relayer]
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
    #[trusted_relayer]
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
            PendingUTXOInfo {
                tx_id,
                utxo_storage_key,
                utxo,
            },
            deposit_msg,
        )
    }

    /// Safe version of verify_deposit, only supports minting nBTC with safe_deposit message and revert the deposit on failed XCC calls.
    /// It doesn't charge deposit fee, and doesn't pay the token storage for the user
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
    #[payable]
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn safe_verify_deposit(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
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
        let utxo_storage_key = generate_utxo_storage_key(tx_id.clone(), vout.try_into().unwrap());

        self.internal_safe_verify_deposit(
            deposit_amount,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            PendingUTXOInfo {
                tx_id,
                utxo_storage_key,
                utxo,
            },
            deposit_msg.recipient_id,
            safe_deposit_msg,
        )
    }

    #[payable]
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn safe_verify_deposit_compact(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Base64VecU8,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        self.safe_verify_deposit(
            deposit_msg,
            tx_bytes.0,
            vout,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
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
    #[trusted_relayer]
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
    pub fn withdraw_rbf(
        &mut self,
        original_btc_pending_verify_id: String,
        output: Vec<TxOut>,
        chain_specific_data: Option<ChainSpecificData>,
    ) {
        let account_id = env::predecessor_account_id();
        self.require_pending_sign_capacity(&account_id);

        self.withdraw_rbf_chain_specific(
            account_id,
            original_btc_pending_verify_id,
            output,
            chain_specific_data,
        );
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
        self.require_pending_sign_capacity(&user_account_id);

        self.cancel_withdraw_chain_specific(
            user_account_id,
            original_btc_pending_verify_id,
            output,
            None,
        );
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
    #[trusted_relayer]
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
        self.active_utxo_management_chain_specific(account_id, input, output);
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
        self.require_pending_sign_capacity(&account_id);
        self.active_utxo_management_rbf_chain_specific(
            account_id,
            original_btc_pending_verify_id,
            output,
            None,
        );
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
        self.require_pending_sign_capacity(&user_account_id);
        self.cancel_active_utxo_management_chain_specific(
            user_account_id,
            original_btc_pending_verify_id,
            output,
            None,
        );
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

#[cfg(not(feature = "zcash"))]
#[trusted_relayer]
#[near]
impl Contract {
    // ── Refund API (Bitcoin only) ──

    /// Submit a refund request for a deposit that was never finalized via `verify_deposit` or `safe_verify_deposit`.
    /// The BTC transaction is verified through the Light Client to prove the deposit exists.
    /// After the timelock period, anyone can call `execute_refund` to initiate the return.
    ///
    /// # Arguments
    ///
    /// * `deposit_msg` - The original deposit message. If `deposit_msg.refund_address` is set,
    ///   it must match the provided `refund_address`.
    /// * `refund_address` - BTC address to send the refund to. If `deposit_msg.refund_address`
    ///   is `None`, this value is used directly.
    /// * `tx_bytes` - BTC transaction bytes proving the deposit.
    /// * `vout` - Output index of the deposit in the transaction.
    /// * `tx_block_blockhash` - Block hash containing the transaction.
    /// * `tx_index` - Transaction index within the block.
    /// * `merkle_proof` - Merkle proof for Light Client verification.
    /// * `gas_fee` - Optional custom gas fee. Only DAO or Operator can set this.
    ///   If `None`, the default `config.max_btc_gas_fee` is used during `execute_refund`.
    #[allow(clippy::too_many_arguments)]
    #[pause(except(roles(Role::DAO)))]
    pub fn request_refund(
        &mut self,
        deposit_msg: DepositMsg,
        refund_address: String,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        gas_fee: Option<U128>,
    ) -> Promise {
        if gas_fee.is_some() {
            let caller = env::predecessor_account_id();
            require!(
                self.acl_has_role(Role::DAO.into(), caller.clone())
                    || self.acl_has_role(Role::Operator.into(), caller),
                "Only DAO or Operator can specify custom gas_fee"
            );
        }
        self.internal_request_refund(
            deposit_msg,
            refund_address,
            tx_bytes,
            vout,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            gas_fee.map(|v| v.0),
        )
    }

    /// Reject a pending refund request.
    /// - DAO or Operator can reject any request.
    /// - Anyone can reject a request if the UTXO has already been verified via `verify_deposit`.
    ///
    /// # Arguments
    ///
    /// * `utxo_storage_key` - The UTXO key identifying the refund request (`{tx_id}@{vout}`).
    pub fn reject_refund(&mut self, utxo_storage_key: String) {
        let caller = env::predecessor_account_id();
        let is_privileged = self.acl_has_role(Role::DAO.into(), caller.clone())
            || self.acl_has_role(Role::Operator.into(), caller);
        let is_already_deposited = self
            .data()
            .verified_deposit_utxo
            .contains(&utxo_storage_key);
        require!(
            is_privileged || is_already_deposited,
            "Only DAO/Operator can reject, or UTXO must be already verified via deposit"
        );
        self.internal_reject_refund(utxo_storage_key);
    }

    /// Execute a refund after the timelock has passed. Builds a BTC transaction
    /// that sends the deposit UTXO back to the `refund_address` specified in the original
    /// `DepositMsg`. Creates a `BTCPendingInfo` entry for the MPC sign pipeline.
    /// Marks the UTXO in `verified_deposit_utxo` to prevent future `verify_deposit`.
    ///
    /// # Arguments
    ///
    /// * `utxo_storage_key` - The UTXO key identifying the refund request (`{tx_id}@{vout}`).
    #[payable]
    #[pause(except(roles(Role::DAO)))]
    pub fn execute_refund(&mut self, utxo_storage_key: String) {
        require!(
            env::attached_deposit() >= self.required_balance_for_execute_refund(),
            "Insufficient deposit for storage"
        );
        let caller = env::predecessor_account_id();
        let is_privileged =
            self.acl_has_any_role(vec![Role::DAO.into(), Role::RefundOperator.into()], caller);
        let refund_request: crate::RefundRequest = self
            .data()
            .refund_requests
            .get(&utxo_storage_key)
            .expect("Refund request not found")
            .into();
        let config = self.internal_config();
        let timelock_sec = if refund_request.deposit_msg().refund_address.is_some() {
            // Pre-authorized refund address: privileged users can fast-track.
            if is_privileged {
                0
            } else {
                config.refund_timelock_sec
            }
        } else {
            // Refund address supplied by caller of `request_refund`: longer
            // timelock to give DAO/Operator time to reject suspicious requests.
            config.unsafe_refund_timelock_sec
        };
        self.internal_execute_refund(utxo_storage_key, timelock_sec);
    }

    /// Verify that the refund BTC transaction has been confirmed on the Bitcoin network.
    /// Cleans up the `BTCPendingInfo` after successful verification.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction ID of the confirmed refund transaction.
    /// * `tx_block_blockhash` - Block hash containing the transaction.
    /// * `tx_index` - Transaction index within the block.
    /// * `merkle_proof` - Merkle proof for Light Client verification.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_refund_finalize(
        &mut self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        btc_pending_info.assert_refund_pending_verify_tx();
        require!(
            btc_pending_info.tx_bytes_with_sign.is_some(),
            "Missing tx_bytes_with_sign"
        );
        self.internal_verify_refund_finalize(
            tx_id,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            btc_pending_info,
        )
    }
}

impl Contract {
    pub fn create_active_utxo_management_pending_info(
        &mut self,
        account_id: AccountId,
        mut psbt: PsbtWrapper,
    ) {
        self.require_pending_sign_capacity(&account_id);

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
            .btc_pending_sign_ids
            .insert(btc_pending_id.clone());
        Event::UtxoRemoved { utxo_storage_keys }.emit();
        Event::GenerateBtcPendingInfo {
            account_id: &account_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }
}
