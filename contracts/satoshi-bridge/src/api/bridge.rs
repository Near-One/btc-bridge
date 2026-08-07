use crate::*;
use near_plugins::{access_control_any, pause};
use near_sdk::json_types::Base64VecU8;

#[trusted_relayer]
#[near]
impl Contract {
    /// Verify that the user has transferred BTC asset to the protocol's designated BTC deposit account,
    /// and mint nBTC to the user's NEAR account. Includes coinbase proof for stronger
    /// transaction inclusion verification.
    ///
    /// The deposit flow is selected by `deposit_msg.safe_deposit`:
    /// * `Some(..)` — safe deposit (e.g. Omni Bridge): charges no fee, reverts the whole
    ///   transaction if minting fails (no lost & found), and the caller must attach NEAR for
    ///   the user's token storage (see `required_balance_for_safe_deposit`).
    /// * `None` — standard deposit: charges the deposit fee, pays the user's storage, and
    ///   routes mint failures to lost & found.
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
    #[payable]
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_deposit_v2(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Base64VecU8,
        vout: usize,
        proof: TxInclusionProof,
    ) -> Promise {
        let coinbase_proof = (proof.coinbase_tx_id, proof.coinbase_merkle_proof);
        if deposit_msg.safe_deposit.is_some() {
            self.internal_safe_verify_deposit_entry(
                deposit_msg,
                tx_bytes.0,
                vout,
                proof.tx_block_blockhash,
                proof.tx_index,
                proof.merkle_proof,
                coinbase_proof,
            )
        } else {
            self.internal_verify_deposit_entry(
                deposit_msg,
                tx_bytes.0,
                vout,
                proof.tx_block_blockhash,
                proof.tx_index,
                proof.merkle_proof,
                coinbase_proof,
            )
        }
    }

    #[access_control_any(roles(Role::MigrationOperator, Role::DAO))]
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_migrate_deposit(
        &mut self,
        tx_bytes: Base64VecU8,
        vout: usize,
        proof: TxInclusionProof,
    ) -> Promise {
        self.internal_verify_migrate_deposit_entry(
            tx_bytes.0,
            vout,
            proof.tx_block_blockhash,
            proof.tx_index,
            proof.merkle_proof,
            Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
        )
    }

    #[payable]
    #[access_control_any(roles(Role::MigrationOperator, Role::DAO))]
    #[pause(except(roles(Role::DAO)))]
    pub fn migrate_to_new_token(
        &mut self,
        new_token: AccountId,
        accounts: Vec<AccountId>,
    ) -> Promise {
        assert_one_yocto();
        self.internal_migrate_to_new_token(new_token, accounts)
    }

    /// Verify that a pending bridge transaction (withdraw, active UTXO management or refund)
    /// has been confirmed on-chain, and finalize it accordingly: burn the corresponding
    /// tokens for withdraw / active UTXO management, or clean up the refund state.
    /// The transaction kind is resolved from the stored pending info by `tx_id`.
    /// Includes coinbase proof for stronger transaction inclusion verification.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - The transaction ID of the successfully on-chain transaction.
    /// * `proof` - Transaction inclusion proof with coinbase verification.
    ///
    /// # Returns
    ///
    /// bool - Whether finalization was successful.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn verify_withdraw_v2(&mut self, tx_id: String, proof: TxInclusionProof) -> Promise {
        match self.internal_unwrap_btc_pending_info(&tx_id).state.clone() {
            PendingInfoState::WithdrawOriginal(_)
            | PendingInfoState::WithdrawUserRbf(_)
            | PendingInfoState::WithdrawCancelRbf(_) => self.internal_verify_withdraw_entry(
                tx_id,
                proof.tx_block_blockhash,
                proof.tx_index,
                proof.merkle_proof,
                Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
            ),
            PendingInfoState::ActiveUtxoManagementOriginal(_)
            | PendingInfoState::ActiveUtxoManagementRbf(_)
            | PendingInfoState::ActiveUtxoManagementCancelRbf(_) => self
                .internal_verify_active_utxo_management_entry(
                    tx_id,
                    proof.tx_block_blockhash,
                    proof.tx_index,
                    proof.merkle_proof,
                    Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
                ),
            PendingInfoState::Refund(_) => self.internal_verify_refund_finalize_entry(tx_id, proof),
        }
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

#[trusted_relayer]
#[near]
impl Contract {
    // ── Refund API ──

    /// Submit a refund request for a deposit that was never finalized via `verify_deposit_v2`.
    /// The BTC transaction is verified through the Light Client to prove the deposit exists.
    /// After the timelock period, anyone can call `execute_refund` to initiate the return.
    ///
    /// Requires an attached deposit of at least `required_balance_for_request_refund()`.
    /// The deposit is NOT refunded — it covers request storage and acts as an anti-spam fee.
    ///
    /// # Arguments
    ///
    /// * `deposit_msg` - The original deposit message. If `deposit_msg.refund_address` is set,
    ///   it must match the provided `refund_address`.
    /// * `refund_address` - BTC address to send the refund to. If `deposit_msg.refund_address`
    ///   is `None`, this value is used directly.
    /// * `tx_bytes` - BTC transaction bytes proving the deposit.
    /// * `vout` - Output index of the deposit in the transaction.
    /// * `proof` - Transaction inclusion proof for Light Client verification, bundling:
    ///   `tx_block_blockhash` (block hash containing the transaction), `tx_index`
    ///   (transaction index within the block), `merkle_proof` (Merkle proof of the
    ///   transaction), and the coinbase fields `coinbase_tx_id` and
    ///   `coinbase_merkle_proof` used to verify the block's coinbase.
    /// * `gas_fee` - Optional custom gas fee. Only DAO or Operator can set this.
    ///   If `None`, the default `config.max_btc_gas_fee` is used during `execute_refund`.
    #[allow(clippy::too_many_arguments)]
    #[payable]
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
    /// - Anyone can reject a request if the UTXO has already been verified via `verify_deposit_v2`
    ///
    /// # Arguments
    ///
    /// * `utxo_storage_key` - The UTXO key identifying the refund request (`{tx_id}@{vout}`).
    pub fn reject_refund(&mut self, utxo_storage_key: String) {
        let caller = env::predecessor_account_id();
        let is_privileged = self.acl_has_role(Role::DAO.into(), caller.clone())
            || self.acl_has_role(Role::Operator.into(), caller);
        // `execute_refund` also inserts the UTXO into `verified_deposit_utxo` (to block a
        // later deposit) while keeping the request with `executed == true`. That membership
        // must NOT open the permissionless reject path, otherwise anyone could cancel an
        // in-flight refund — so only treat the UTXO as "already deposited" when the request
        // was not executed by us, i.e. a real `verify_deposit_v2` finalized it.
        let executed = self
            .data()
            .refund_requests
            .get(&utxo_storage_key)
            .map(|r| RefundRequest::from(r).executed)
            .unwrap_or(false);
        let is_already_deposited = !executed
            && self
                .data()
                .verified_deposit_utxo
                .contains(&utxo_storage_key);
        require!(
            is_privileged || is_already_deposited,
            "Only DAO/Operator can reject, or UTXO must be already verified via deposit"
        );
        self.internal_reject_refund(utxo_storage_key);
    }

    /// Execute a refund: send the deposit UTXO back to the original
    /// `refund_address` via the MPC sign pipeline. Requires the timelock to have
    /// passed (bypassed for a privileged caller with a pre-authorized address).
    ///
    /// # Arguments
    ///
    /// * `utxo_storage_key` - Refund request key (`{tx_id}@{vout}`).
    /// * `chain_specific_data` - Zcash only: `Some` with an Orchard bundle for a
    ///   shielded refund, `None` for transparent. Ignored on Bitcoin.
    #[payable]
    #[pause(except(roles(Role::DAO)))]
    pub fn execute_refund(
        &mut self,
        utxo_storage_key: String,
        chain_specific_data: Option<ChainSpecificData>,
    ) -> PromiseOrValue<()> {
        let timelock_sec = self.resolve_execute_refund_timelock(&utxo_storage_key);
        self.internal_execute_refund(utxo_storage_key, timelock_sec, chain_specific_data)
    }

    /// Remove a leftover refund pending transaction whose refund request is gone
    /// (the refund was already finalized via another candidate, or rejected). Such
    /// a transaction can never confirm, so this only cleans up stale state — it is
    /// rejected while the refund request still exists.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Pending id of the stale refund transaction to remove.
    #[trusted_relayer]
    #[pause(except(roles(Role::DAO)))]
    pub fn remove_refund_pending_tx_id(&mut self, tx_id: String) {
        self.internal_remove_refund_pending_tx_id(tx_id);
    }
}
