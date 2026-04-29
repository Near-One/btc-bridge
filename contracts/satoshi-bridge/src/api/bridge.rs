use crate::*;
use near_plugins::{access_control_any, pause};

#[trusted_relayer]
#[near]
impl Contract {
    /// Verify that the user has transferred BTC asset to the protocol's designated BTC deposit account,
    /// and mint NBTC to the user's NEAR account.
    ///
    /// # Deprecated
    /// Use `verify_deposit_v2` instead, which includes coinbase proof for stronger verification.
    ///
    /// # Arguments
    ///
    /// * `deposit_msg` - Information used to generate the deposit address path.
    /// * `tx_bytes` - Successfully confirmed BTC transaction bytes.
    /// * `vout` - The index of the output where the user sent BTC to the deposit address.
    /// * `tx_block_blockhash` - The block hash where the transaction is located.
    /// * `tx_index` - The index of the transaction in the block.
    /// * `merkle_proof` - Merkle proof of the transaction.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC minting was successful.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    #[deprecated(note = "use verify_deposit_v2")]
    pub fn verify_deposit(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        self.internal_verify_deposit_entry(
            deposit_msg,
            Base64VecU8(tx_bytes),
            vout,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            None,
        )
    }

    /// Verify that the user has transferred BTC asset to the protocol's designated BTC deposit account,
    /// and mint NBTC to the user's NEAR account.
    /// Includes coinbase proof for stronger transaction inclusion verification.
    ///
    /// # Arguments
    ///
    /// * `deposit_msg` - Information used to generate the deposit address path.
    /// * `tx_bytes` - Successfully confirmed BTC transaction bytes.
    /// * `vout` - The index of the output where the user sent BTC to the deposit address.
    /// * `proof` - Transaction inclusion proof with coinbase verification.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC minting was successful.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_deposit_v2(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Base64VecU8,
        vout: usize,
        proof: TxInclusionProof,
    ) -> Promise {
        self.internal_verify_deposit_entry(
            deposit_msg,
            tx_bytes,
            vout,
            proof.tx_block_blockhash,
            proof.tx_index,
            proof.merkle_proof,
            Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
        )
    }

    /// Safe version of verify_deposit, only supports minting nBTC with safe_deposit message and revert the deposit on failed XCC calls.
    /// It doesn't charge deposit fee, and doesn't pay the token storage for the user
    ///
    /// # Deprecated
    /// Use `safe_verify_deposit_v2` instead, which includes coinbase proof for stronger verification.
    ///
    /// # Arguments
    ///
    /// * `deposit_msg` - Information used to generate the deposit address path. Must contain `safe_deposit`.
    /// * `tx_bytes` - Successfully confirmed BTC transaction bytes.
    /// * `vout` - The index of the output where the user sent BTC to the deposit address.
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
    #[deprecated(note = "use safe_verify_deposit_v2")]
    pub fn safe_verify_deposit(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        self.internal_safe_verify_deposit_entry(
            deposit_msg,
            Base64VecU8(tx_bytes),
            vout,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            None,
        )
    }

    /// Safe version of verify_deposit. Reverts the entire transaction if mint fails (no lost & found).
    /// Does not charge deposit fees. User must attach NEAR for storage.
    /// Includes coinbase proof for stronger transaction inclusion verification.
    ///
    /// # Arguments
    ///
    /// * `deposit_msg` - Information used to generate the deposit address path. Must contain `safe_deposit`.
    /// * `tx_bytes` - Successfully confirmed BTC transaction bytes.
    /// * `vout` - The index of the output where the user sent BTC to the deposit address.
    /// * `proof` - Transaction inclusion proof with coinbase verification.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC minting was successful.
    #[payable]
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn safe_verify_deposit_v2(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Base64VecU8,
        vout: usize,
        proof: TxInclusionProof,
    ) -> Promise {
        self.internal_safe_verify_deposit_entry(
            deposit_msg,
            tx_bytes,
            vout,
            proof.tx_block_blockhash,
            proof.tx_index,
            proof.merkle_proof,
            Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
        )
    }

    /// Verify that the user's withdrawal has been successful, and burn the corresponding amount of tokens.
    ///
    /// # Deprecated
    /// Use `verify_withdraw_v2` instead, which includes coinbase proof for stronger verification.
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
    #[deprecated(note = "use verify_withdraw_v2")]
    pub fn verify_withdraw(
        &mut self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        self.internal_verify_withdraw_entry(tx_id, tx_block_blockhash, tx_index, merkle_proof, None)
    }

    /// Verify that the user's withdrawal has been successful, and burn the corresponding amount of tokens.
    /// Includes coinbase proof for stronger transaction inclusion verification.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - The transaction ID of the successfully on-chain withdrawal.
    /// * `proof` - Transaction inclusion proof with coinbase verification.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC burning was successful.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_withdraw_v2(&mut self, tx_id: String, proof: TxInclusionProof) -> Promise {
        self.internal_verify_withdraw_entry(
            tx_id,
            proof.tx_block_blockhash,
            proof.tx_index,
            proof.merkle_proof,
            Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
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

    /// Verify that the active UTXO management has been successful, and burn the gas fee.
    ///
    /// # Deprecated
    /// Use `verify_active_utxo_management_v2` instead, which includes coinbase proof for stronger verification.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - The transaction ID of the successfully on-chain UTXO management.
    /// * `tx_block_blockhash` - The block hash where the transaction is located.
    /// * `tx_index` - The index of the transaction in the block.
    /// * `merkle_proof` - Merkle proof of the transaction.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC burning was successful.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    #[deprecated(note = "use verify_active_utxo_management_v2")]
    pub fn verify_active_utxo_management(
        &mut self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        self.internal_verify_active_utxo_management_entry(
            tx_id,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            None,
        )
    }

    /// Verify that the active UTXO management has been successful, and burn the gas fee.
    /// Includes coinbase proof for stronger transaction inclusion verification.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - The transaction ID of the successfully on-chain UTXO management.
    /// * `proof` - Transaction inclusion proof with coinbase verification.
    ///
    /// # Returns
    ///
    /// bool - Whether nBTC burning was successful.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_active_utxo_management_v2(
        &mut self,
        tx_id: String,
        proof: TxInclusionProof,
    ) -> Promise {
        self.internal_verify_active_utxo_management_entry(
            tx_id,
            proof.tx_block_blockhash,
            proof.tx_index,
            proof.merkle_proof,
            Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
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
        tx_bytes: Base64VecU8,
        vout: usize,
        proof: TxInclusionProof,
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
            proof,
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
        let has_refund_address = refund_request.deposit_msg().refund_address.is_some();
        let skip_timelock = is_privileged && has_refund_address;
        self.internal_execute_refund(utxo_storage_key, skip_timelock);
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
    pub fn verify_refund_finalize(&mut self, tx_id: String, proof: TxInclusionProof) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        btc_pending_info.assert_refund_pending_verify_tx();
        require!(
            btc_pending_info.tx_bytes_with_sign.is_some(),
            "Missing tx_bytes_with_sign"
        );
        self.internal_verify_refund_finalize(tx_id, proof, btc_pending_info)
    }
}
