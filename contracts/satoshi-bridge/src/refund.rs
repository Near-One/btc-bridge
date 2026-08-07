use bitcoin::{Amount, OutPoint, TxOut};
use near_sdk::json_types::Base64VecU8;

use crate::{
    btc_light_client::TxInclusionInfo, env, near, require, serde_json, AccessControllable,
    AccountId, BTCPendingInfo, Contract, ContractExt, DepositMsg, Event, Gas, OriginalState,
    PendingInfoStage, PendingInfoState, Promise, Role, TxInclusionProof, MAX_BOOL_RESULT,
    MAX_INCLUSION_INFO_RESULT, UTXO, VUTXO,
};

use crate::deposit_msg::get_deposit_path;
use crate::psbt_wrapper::PsbtWrapper;
use crate::utils::{generate_utxo_storage_key, nano_to_sec};

pub(crate) const GAS_FOR_REQUEST_REFUND_CALLBACK: Gas = Gas::from_tgas(20);
pub(crate) const GAS_FOR_VERIFY_REFUND_CALLBACK: Gas = Gas::from_tgas(20);

/// Upper bound on the deposit `tx_bytes` accepted by `request_refund`.
///
/// The RefundRequest stores `tx_bytes` verbatim (no truncation — `execute_refund`
/// later decodes them to rebuild the refund tx), so storage grows ~1:1 with tx size:
/// at this cap a request stores ~200 KB ≈ 2 NEAR, which `required_balance_for_request_refund`
/// is sized to cover. The cap also sits safely below the hard gas ceiling: decoding +
/// borsh-storing the tx happens in `request_refund_callback` (only 20 Tgas), which runs
/// out of gas around ~250 KB regardless of the attached deposit. 200 KB is ~1350 signed
/// P2PKH inputs — far above any real deposit (1-2 inputs), incl. large consolidations.
pub(crate) const MAX_REQUEST_REFUND_TX_BYTES: usize = 200_000;

/// Stored refund request. `deposit_msg` is kept as JSON string
/// because `DepositMsg` does not implement Borsh serialization.
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct RefundRequest {
    pub deposit_msg_json: String,
    pub utxo_storage_key: String,
    pub tx_bytes: Base64VecU8,
    pub vout: usize,
    pub amount: u128,
    pub refund_address: String,
    pub gas_fee: u128,
    pub created_at_sec: u32,
    /// Set once `execute_refund` has built a refund transaction for this request.
    /// While `true` the request is kept (not removed) so `execute_refund` can be
    /// called again to re-create the transaction (e.g. after a consensus branch
    /// change); it is removed only when the refund is finalized via
    /// `verify_withdraw_v2`.
    pub executed: bool,
}

impl RefundRequest {
    pub fn deposit_msg(&self) -> DepositMsg {
        serde_json::from_str(&self.deposit_msg_json).expect("Invalid deposit_msg_json")
    }
}

/// Refund request as stored before the `executed` field was added (the deployed
/// 8-field layout). Kept as the `V0` variant of [`VRefundRequest`] so existing
/// on-chain entries deserialize and are upgraded lazily on read/insert.
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct RefundRequestV0 {
    pub deposit_msg_json: String,
    pub utxo_storage_key: String,
    pub tx_bytes: Base64VecU8,
    pub vout: usize,
    pub amount: u128,
    pub refund_address: String,
    pub gas_fee: u128,
    pub created_at_sec: u32,
}

impl From<RefundRequestV0> for RefundRequest {
    fn from(v: RefundRequestV0) -> Self {
        RefundRequest {
            deposit_msg_json: v.deposit_msg_json,
            utxo_storage_key: v.utxo_storage_key,
            tx_bytes: v.tx_bytes,
            vout: v.vout,
            amount: v.amount,
            refund_address: v.refund_address,
            gas_fee: v.gas_fee,
            created_at_sec: v.created_at_sec,
            // Pre-`executed` requests were removed on finalize, so any persisted
            // one was still pending.
            executed: false,
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub enum VRefundRequest {
    /// Deployed 8-field layout (no `executed`). Variant tag 0 — must stay first
    /// so existing on-chain values keep deserializing.
    V0(RefundRequestV0),
    Current(RefundRequest),
}

impl From<VRefundRequest> for RefundRequest {
    fn from(v: VRefundRequest) -> Self {
        match v {
            VRefundRequest::V0(c) => c.into(),
            VRefundRequest::Current(c) => c,
        }
    }
}

impl From<&VRefundRequest> for RefundRequest {
    fn from(v: &VRefundRequest) -> Self {
        match v {
            VRefundRequest::V0(c) => c.clone().into(),
            VRefundRequest::Current(c) => c.clone(),
        }
    }
}

impl From<RefundRequest> for VRefundRequest {
    fn from(c: RefundRequest) -> Self {
        VRefundRequest::Current(c)
    }
}

/// Inputs derived from the original deposit transaction, needed to build a refund.
pub(crate) struct RefundExecutionInputs {
    /// The deposit UTXO being spent by the refund.
    pub outpoint: OutPoint,
    /// The deposit output (used as the input witness amount/script).
    pub deposit_output: TxOut,
    /// Amount returned to the user: deposit value minus the gas fee.
    pub refund_amount: u128,
}

impl Contract {
    /// Submit a refund request. Verifies the BTC transaction via Light Client first.
    /// If `deposit_msg.refund_address` is set, it must match the provided `refund_address`.
    /// If `deposit_msg.refund_address` is None, the provided `refund_address` is used.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn internal_request_refund(
        &self,
        deposit_msg: DepositMsg,
        refund_address: String,
        tx_bytes: Base64VecU8,
        vout: usize,
        proof: TxInclusionProof,
        gas_fee: Option<u128>,
    ) -> Promise {
        require!(
            env::attached_deposit() >= self.required_balance_for_request_refund(),
            "Insufficient deposit for storage"
        );
        require!(
            tx_bytes.0.len() <= MAX_REQUEST_REFUND_TX_BYTES,
            "tx_bytes too large for refund request"
        );
        if let Some(msg_refund_address) = &deposit_msg.refund_address {
            require!(
                msg_refund_address == &refund_address,
                "refund_address does not match deposit_msg.refund_address"
            );
        }

        let transaction =
            crate::WrappedTransaction::decode(&tx_bytes.0, &self.internal_config().chain)
                .expect("Deserialization tx_bytes failed");
        let tx_id = transaction.compute_txid().to_string();

        // Refunds skip the block-amount ring; max-tier depth is required unconditionally.
        let config = self.internal_config();
        self.verify_transaction_inclusion_with_heights_promise(
            config.btc_light_client_account_id.clone(),
            tx_id,
            proof.tx_block_blockhash,
            proof.tx_index,
            proof.merkle_proof,
            (proof.coinbase_tx_id, proof.coinbase_merkle_proof),
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_REQUEST_REFUND_CALLBACK)
                .request_refund_callback(deposit_msg, refund_address, tx_bytes, vout, gas_fee),
        )
    }

    /// Reject a pending refund request.
    pub(crate) fn internal_reject_refund(&mut self, utxo_storage_key: String) {
        require!(
            self.data_mut()
                .refund_requests
                .remove(&utxo_storage_key)
                .is_some(),
            "Refund request not found"
        );
        Event::RefundRejected { utxo_storage_key }.emit();
    }

    /// Validate the attached storage deposit and resolve the timelock that must
    /// elapse before this refund can be executed. Shared by the Bitcoin and
    /// Zcash `execute_refund` entrypoints.
    pub(crate) fn resolve_execute_refund_timelock(&self, utxo_storage_key: &str) -> u64 {
        require!(
            env::attached_deposit() >= self.required_balance_for_execute_refund(),
            "Insufficient deposit for storage"
        );
        let caller = env::predecessor_account_id();
        let is_privileged =
            self.acl_has_any_role(vec![Role::DAO.into(), Role::RefundOperator.into()], caller);
        let refund_request: RefundRequest = self
            .data()
            .refund_requests
            .get(utxo_storage_key)
            .expect("Refund request not found")
            .into();
        let config = self.internal_config();
        if refund_request.deposit_msg().refund_address.is_some() {
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
        }
    }

    /// Load a refund request and run the common pre-execution checks
    /// (timelock elapsed, not already finalized via deposit).
    pub(crate) fn load_refund_request_for_execute(
        &self,
        utxo_storage_key: &str,
        timelock_sec: u64,
    ) -> RefundRequest {
        let refund_request: RefundRequest = self
            .data()
            .refund_requests
            .get(utxo_storage_key)
            .expect("Refund request not found")
            .into();

        let now = nano_to_sec(env::block_timestamp());
        require!(
            u64::from(now) >= u64::from(refund_request.created_at_sec) + timelock_sec,
            "Refund timelock has not passed yet"
        );

        // Block only if the UTXO was claimed by a deposit. If it was claimed by
        // our own refund (executed == true, which also set verified_deposit_utxo),
        // re-running execute_refund is allowed — re-creating the refund tx, e.g.
        // after a consensus branch change.
        require!(
            !self.data().verified_deposit_utxo.contains(utxo_storage_key)
                || refund_request.executed,
            "UTXO already verified via deposit, cannot refund"
        );

        refund_request
    }

    /// Parse the original deposit transaction and compute the refund economics.
    pub(crate) fn refund_execution_inputs(
        &self,
        refund_request: &RefundRequest,
    ) -> RefundExecutionInputs {
        let config = self.internal_config();
        let transaction =
            crate::WrappedTransaction::decode(&refund_request.tx_bytes.0, &config.chain)
                .expect("Deserialization tx_bytes failed");
        let txid = transaction.compute_txid();
        let outpoint = OutPoint {
            txid,
            vout: u32::try_from(refund_request.vout)
                .unwrap_or_else(|_| env::panic_str("vout overflow")),
        };
        let deposit_output = transaction.output()[refund_request.vout].clone();

        let refund_amount = refund_request
            .amount
            .checked_sub(refund_request.gas_fee)
            .expect("Deposit amount too small to cover gas fee");
        require!(refund_amount > 0, "Refund amount is zero after gas fee");

        RefundExecutionInputs {
            outpoint,
            deposit_output,
            refund_amount,
        }
    }

    /// Build a transparent refund output paying `refund_amount` to `refund_address`.
    pub(crate) fn build_refund_output(&self, refund_address: &str, refund_amount: u128) -> TxOut {
        let config = self.internal_config();
        let refund_addr = crate::network::Address::parse(refund_address, config.chain.clone())
            .expect("Invalid refund address");
        let refund_script_pubkey = refund_addr
            .script_pubkey()
            .expect("Invalid refund script_pubkey");
        TxOut {
            value: Amount::from_sat(
                u64::try_from(refund_amount)
                    .unwrap_or_else(|_| env::panic_str("Refund amount overflow")),
            ),
            script_pubkey: refund_script_pubkey,
        }
    }

    /// Given a fully-built refund PSBT, create the refund `BTCPendingInfo`, mark
    /// the deposit UTXO verified (to block a later `verify_deposit_v2`), emit events
    /// and remove the request. `caller` is the account that will own the pending
    /// info — it must be passed explicitly because on Zcash this runs inside a
    /// `#[private]` callback where `predecessor` is the contract itself.
    pub(crate) fn finalize_refund_with_psbt(
        &mut self,
        caller: AccountId,
        mut refund_request: RefundRequest,
        psbt: PsbtWrapper,
        refund_amount: u128,
        utxo_storage_key: String,
    ) {
        let gas_fee = refund_request.gas_fee;
        let refund_address = refund_request.refund_address.clone();

        let deposit_msg = refund_request.deposit_msg();
        let path = get_deposit_path(&deposit_msg);
        let vutxo = VUTXO::Current(UTXO {
            path,
            tx_bytes: refund_request.tx_bytes.0.clone(),
            vout: refund_request.vout,
            balance: u64::try_from(refund_request.amount)
                .unwrap_or_else(|_| env::panic_str("Amount overflow")),
        });

        let psbt_hex = psbt.serialize();
        let btc_pending_id = psbt.get_pending_id();

        if !self.check_account_exists(&caller) {
            self.internal_set_account(&caller, crate::Account::new(&caller));
        }
        self.require_pending_sign_capacity(&caller);

        let btc_pending_info = BTCPendingInfo {
            account_id: caller.clone(),
            btc_pending_id: btc_pending_id.clone(),
            transfer_amount: 0,
            actual_received_amount: refund_amount,
            withdraw_fee: 0,
            gas_fee,
            burn_amount: 0,
            psbt_hex,
            vutxos: vec![vutxo],
            signatures: vec![None; 1],
            tx_bytes_with_sign: None,
            create_time_sec: nano_to_sec(env::block_timestamp()),
            last_sign_time_sec: 0,
            state: PendingInfoState::Refund(OriginalState {
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
        self.internal_unwrap_mut_account(&caller)
            .btc_pending_sign_ids
            .insert(btc_pending_id.clone());

        // Mark UTXO as verified to prevent verify_deposit_v2 later
        self.data_mut()
            .verified_deposit_utxo
            .insert(utxo_storage_key.clone());

        Event::RefundExecuted {
            utxo_storage_key: utxo_storage_key.clone(),
            amount: refund_request.amount.into(),
            refund_address,
        }
        .emit();

        Event::GenerateBtcPendingInfo {
            account_id: &caller,
            btc_pending_id: &btc_pending_id,
        }
        .emit();

        // Keep the request (so `execute_refund` can be called again to re-create
        // the transaction) but mark it executed; it is removed only when the
        // refund is finalized via `verify_withdraw_v2`.
        refund_request.executed = true;
        self.data_mut()
            .refund_requests
            .insert(utxo_storage_key, refund_request.into());
    }

    /// Remove a leftover refund pending transaction. Only allowed once its refund
    /// request is gone — i.e. the refund was finalized via another candidate or
    /// rejected — in which case this pending tx can never confirm (its UTXO is
    /// spent or the refund was cancelled) and is just stale state to clean up.
    pub(crate) fn internal_remove_refund_pending_tx_id(&mut self, tx_id: String) {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id).clone();
        btc_pending_info.assert_refund_related();

        // A refund spends exactly one deposit UTXO, whose key is the refund request key.
        let utxo_storage_keys = btc_pending_info.get_psbt().get_utxo_storage_keys();
        require!(
            utxo_storage_keys.len() == 1,
            "refund transaction must spend exactly one input"
        );
        require!(
            !self
                .data()
                .refund_requests
                .contains_key(&utxo_storage_keys[0]),
            "refund request still active"
        );

        let account_id = btc_pending_info.account_id.clone();
        self.internal_remove_btc_pending_info(&tx_id);
        let account = self.internal_unwrap_mut_account(&account_id);
        account.btc_pending_sign_ids.remove(&tx_id);
        account.btc_pending_verify_list.remove(&tx_id);
    }

    pub(crate) fn internal_verify_refund_finalize_entry(
        &mut self,
        tx_id: String,
        proof: TxInclusionProof,
    ) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        btc_pending_info.assert_refund_pending_verify_tx();
        require!(
            btc_pending_info.tx_bytes_with_sign.is_some(),
            "Missing tx_bytes_with_sign"
        );
        self.internal_verify_refund_finalize(tx_id, proof, btc_pending_info)
    }

    /// Verify refund transaction was included in Bitcoin blockchain.
    pub(crate) fn internal_verify_refund_finalize(
        &self,
        tx_id: String,
        proof: TxInclusionProof,
        btc_pending_info: &BTCPendingInfo,
    ) -> Promise {
        let config = self.internal_config();
        let confirmations = config.get_confirmations(btc_pending_info.actual_received_amount)
            + self.relayer_delta_for_predecessor();
        self.verify_transaction_inclusion_promise(
            config.btc_light_client_account_id.clone(),
            tx_id.clone(),
            proof.tx_block_blockhash,
            proof.tx_index,
            proof.merkle_proof,
            Some((proof.coinbase_tx_id, proof.coinbase_merkle_proof)),
            confirmations,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_VERIFY_REFUND_CALLBACK)
                .verify_refund_finalize_callback(tx_id),
        )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn verify_refund_finalize_callback(&mut self, tx_id: String) -> bool {
        let result_bytes = env::promise_result_checked(0, MAX_BOOL_RESULT)
            .expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");

        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id).clone();
        btc_pending_info.assert_refund_pending_verify_tx();

        let account_id = btc_pending_info.account_id.clone();

        // A refund spends exactly one deposit UTXO, whose key is the refund request
        // key. More than one input would be abnormal for a refund.
        let utxo_storage_keys = btc_pending_info.get_psbt().get_utxo_storage_keys();
        require!(
            utxo_storage_keys.len() == 1,
            "refund transaction must spend exactly one input"
        );
        // Refund confirmed on-chain → drop the request so no further execute_refund
        // is possible. If it was already removed, this is harmlessly a no-op.
        self.data_mut()
            .refund_requests
            .remove(&utxo_storage_keys[0]);

        // Clean up: remove pending info
        self.internal_remove_btc_pending_info(&tx_id);
        self.internal_unwrap_mut_account(&account_id)
            .btc_pending_verify_list
            .remove(&tx_id);

        true
    }

    #[private]
    pub fn request_refund_callback(
        &mut self,
        deposit_msg: DepositMsg,
        refund_address: String,
        tx_bytes: Base64VecU8,
        vout: usize,
        gas_fee: Option<u128>,
    ) -> bool {
        let result_bytes = env::promise_result_checked(0, MAX_INCLUSION_INFO_RESULT)
            .expect("Call verify_transaction_inclusion_with_heights failed");
        let info: Option<TxInclusionInfo> = serde_json::from_slice(&result_bytes)
            .expect("verify_transaction_inclusion_with_heights returned an unexpected payload");
        let info = info.expect("Transaction not included in the BTC mainchain");

        let config = self.internal_config();
        let required = config.max_required_confirmations();
        let actual = info
            .mainchain_tip_height
            .saturating_sub(info.tx_block_height)
            + 1;
        require!(
            actual >= required,
            "Refund request: not enough confirmations (max-tier required)"
        );
        let transaction = crate::WrappedTransaction::decode(&tx_bytes.0, &config.chain)
            .expect("Deserialization tx_bytes failed");
        let output = &transaction.output()[vout];

        // Verify that the output script matches the deposit address derived from deposit_msg
        let path = get_deposit_path(&deposit_msg);
        let deposit_address = self.generate_utxo_chain_address(&path);
        let deposit_script_pubkey = deposit_address
            .script_pubkey()
            .expect("Invalid deposit address");
        require!(
            deposit_script_pubkey == output.script_pubkey,
            "Output script_pubkey does not match deposit address"
        );

        let amount = u128::from(output.value.to_sat());
        let tx_id = transaction.compute_txid().to_string();
        let utxo_storage_key = generate_utxo_storage_key(
            tx_id,
            u32::try_from(vout).unwrap_or_else(|_| env::panic_str("vout overflow")),
        );

        // Double-check not finalized (could have been verified between request and callback)
        require!(
            !self
                .data()
                .verified_deposit_utxo
                .contains(&utxo_storage_key),
            "UTXO already verified via deposit"
        );

        // Double-check no duplicate (another request_refund could have landed between our check and callback)
        require!(
            !self.data().refund_requests.contains_key(&utxo_storage_key),
            "Refund request already exists for this UTXO"
        );

        let resolved_gas_fee = gas_fee.unwrap_or_else(|| self.get_refund_gas_fee());
        require!(
            resolved_gas_fee < amount,
            "Gas fee must be less than deposit amount"
        );

        Event::RefundRequested {
            deposit_msg: deposit_msg.clone(),
            utxo_storage_key: utxo_storage_key.clone(),
            amount: amount.into(),
            refund_address: refund_address.clone(),
            gas_fee: resolved_gas_fee.into(),
        }
        .emit();

        let refund_request = RefundRequest {
            deposit_msg_json: serde_json::to_string(&deposit_msg).unwrap(),
            utxo_storage_key: utxo_storage_key.clone(),
            tx_bytes,
            vout,
            amount,
            refund_address,
            gas_fee: resolved_gas_fee,
            created_at_sec: nano_to_sec(env::block_timestamp()),
            executed: false,
        };

        self.data_mut()
            .refund_requests
            .insert(utxo_storage_key, refund_request.into());

        true
    }
}
