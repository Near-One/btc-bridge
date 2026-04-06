use bitcoin::{Amount, OutPoint, TxOut};

use crate::{
    env, near, require, serde_json, BTCPendingInfo, Contract, ContractExt, DepositMsg, Event, Gas,
    OriginalState, PendingInfoStage, PendingInfoState, Promise, UTXO, VUTXO,
};

use crate::deposit_msg::get_deposit_path;
use crate::psbt_wrapper::PsbtWrapper;
use crate::utils::{generate_utxo_storage_key, nano_to_sec};

pub const GAS_FOR_REQUEST_REFUND_CALLBACK: Gas = Gas::from_tgas(20);
pub const GAS_FOR_VERIFY_REFUND_CALLBACK: Gas = Gas::from_tgas(20);

/// Stored refund request. `deposit_msg` is kept as JSON string
/// because `DepositMsg` does not implement Borsh serialization.
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct RefundRequest {
    pub deposit_msg_json: String,
    pub utxo_storage_key: String,
    pub tx_bytes: Vec<u8>,
    pub vout: usize,
    pub amount: u128,
    pub refund_address: String,
    pub created_at_sec: u32,
}

impl RefundRequest {
    pub fn deposit_msg(&self) -> DepositMsg {
        serde_json::from_str(&self.deposit_msg_json).expect("Invalid deposit_msg_json")
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub enum VRefundRequest {
    Current(RefundRequest),
}

impl From<VRefundRequest> for RefundRequest {
    fn from(v: VRefundRequest) -> Self {
        match v {
            VRefundRequest::Current(c) => c,
        }
    }
}

impl From<&VRefundRequest> for RefundRequest {
    fn from(v: &VRefundRequest) -> Self {
        match v {
            VRefundRequest::Current(c) => c.clone(),
        }
    }
}

impl From<RefundRequest> for VRefundRequest {
    fn from(c: RefundRequest) -> Self {
        VRefundRequest::Current(c)
    }
}

impl Contract {
    /// Submit a refund request. Verifies the BTC transaction via Light Client first.
    pub fn internal_request_refund(
        &self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Promise {
        require!(
            deposit_msg.refund_address.is_some(),
            "DepositMsg must contain refund_address"
        );

        let transaction =
            crate::WrappedTransaction::decode(&tx_bytes, &self.internal_config().chain)
                .expect("Deserialization tx_bytes failed");
        let tx_id = transaction.compute_txid().to_string();
        let utxo_storage_key = generate_utxo_storage_key(
            tx_id.clone(),
            u32::try_from(vout).unwrap_or_else(|_| env::panic_str("vout overflow")),
        );

        // Must not be already verified/finalized
        require!(
            !self
                .data()
                .verified_deposit_utxo
                .contains(&utxo_storage_key),
            "UTXO already verified via deposit"
        );

        // Must not have a pending refund request already
        require!(
            !self.data().refund_requests.contains_key(&utxo_storage_key),
            "Refund request already exists for this UTXO"
        );

        let config = self.internal_config();
        let confirmations = self.get_confirmations(config, 0);

        self.verify_transaction_inclusion_promise(
            config.btc_light_client_account_id.clone(),
            tx_id,
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            confirmations,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_REQUEST_REFUND_CALLBACK)
                .request_refund_callback(deposit_msg, tx_bytes, vout),
        )
    }

    /// Reject a pending refund request.
    pub fn internal_reject_refund(&mut self, utxo_storage_key: String) {
        require!(
            self.data_mut()
                .refund_requests
                .remove(&utxo_storage_key)
                .is_some(),
            "Refund request not found"
        );
        Event::RefundRejected { utxo_storage_key }.emit();
    }

    /// Execute an approved refund request after timelock has passed.
    pub fn internal_execute_refund(&mut self, utxo_storage_key: String) {
        let refund_request: RefundRequest = self
            .data()
            .refund_requests
            .get(&utxo_storage_key)
            .expect("Refund request not found")
            .into();

        let config = self.internal_config();

        // Check timelock
        let now = nano_to_sec(env::block_timestamp());
        require!(
            u64::from(now) >= u64::from(refund_request.created_at_sec) + config.refund_timelock_sec,
            "Refund timelock has not passed yet"
        );

        // Must still not be finalized
        require!(
            !self
                .data()
                .verified_deposit_utxo
                .contains(&utxo_storage_key),
            "UTXO already verified via deposit, cannot refund"
        );

        let refund_address = refund_request.refund_address.clone();

        // Parse the original deposit transaction to get OutPoint
        let transaction =
            crate::WrappedTransaction::decode(&refund_request.tx_bytes, &config.chain)
                .expect("Deserialization tx_bytes failed");
        let txid = transaction.compute_txid();
        let outpoint = OutPoint {
            txid,
            vout: u32::try_from(refund_request.vout)
                .unwrap_or_else(|_| env::panic_str("vout overflow")),
        };

        // The deposit UTXO output (for witness)
        let deposit_output = transaction.output()[refund_request.vout].clone();

        // Parse refund address
        let refund_addr = crate::network::Address::parse(&refund_address, config.chain.clone())
            .expect("Invalid refund address");
        let refund_script_pubkey = refund_addr
            .script_pubkey()
            .expect("Invalid refund script_pubkey");

        // Calculate gas fee: entire remainder goes to gas
        let gas_fee = config.max_btc_gas_fee;
        let refund_amount = refund_request
            .amount
            .checked_sub(gas_fee)
            .expect("Deposit amount too small to cover gas fee");
        require!(refund_amount > 0, "Refund amount is zero after gas fee");

        // Build refund output
        let refund_output = TxOut {
            value: Amount::from_sat(
                u64::try_from(refund_amount)
                    .unwrap_or_else(|_| env::panic_str("Refund amount overflow")),
            ),
            script_pubkey: refund_script_pubkey,
        };

        // Build PSBT: 1 input (deposit UTXO), 1 output (refund address)
        let mut psbt = PsbtWrapper::new(vec![outpoint], vec![refund_output]);
        psbt.set_input_utxo(vec![deposit_output]);

        // Build VUTXO for signing (path derived from deposit_msg)
        let deposit_msg = refund_request.deposit_msg();
        let path = get_deposit_path(&deposit_msg);
        let vutxo = VUTXO::Current(UTXO {
            path,
            tx_bytes: refund_request.tx_bytes.clone(),
            vout: refund_request.vout,
            balance: u64::try_from(refund_request.amount)
                .unwrap_or_else(|_| env::panic_str("Amount overflow")),
        });

        // Create BTCPendingInfo
        let psbt_hex = psbt.serialize();
        let btc_pending_id = psbt.get_pending_id();
        let caller = env::predecessor_account_id();

        if !self.check_account_exists(&caller) {
            self.internal_set_account(&caller, crate::Account::new(&caller));
        }
        require!(
            self.internal_unwrap_account(&caller)
                .btc_pending_sign_id
                .is_none(),
            "Previous btc tx has not been signed"
        );

        let btc_pending_info = BTCPendingInfo {
            account_id: caller.clone(),
            btc_pending_id: btc_pending_id.clone(),
            transfer_amount: 0,
            actual_received_amount: refund_amount,
            withdraw_fee: 0,
            gas_fee,
            burn_amount: gas_fee,
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
            .btc_pending_sign_id = Some(btc_pending_id.clone());

        // Mark UTXO as verified to prevent verify_deposit later
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

        self.data_mut().refund_requests.remove(&utxo_storage_key);
    }

    /// Verify refund transaction was included in Bitcoin blockchain.
    pub fn internal_verify_refund(
        &self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
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
            confirmations,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_VERIFY_REFUND_CALLBACK)
                .verify_refund_callback(tx_id),
        )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn verify_refund_callback(&mut self, tx_id: String) -> bool {
        let result_bytes =
            crate::promise_result_as_success().expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");

        let btc_pending_info = self.internal_unwrap_btc_pending_info(&tx_id);
        btc_pending_info.assert_refund_pending_verify_tx();

        let account_id = btc_pending_info.account_id.clone();

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
        tx_bytes: Vec<u8>,
        vout: usize,
    ) -> bool {
        let result_bytes =
            crate::promise_result_as_success().expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");

        let config = self.internal_config();
        let transaction = crate::WrappedTransaction::decode(&tx_bytes, &config.chain)
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

        let refund_address = deposit_msg
            .refund_address
            .clone()
            .expect("No refund address");

        Event::RefundRequested {
            deposit_msg: deposit_msg.clone(),
            utxo_storage_key: utxo_storage_key.clone(),
            amount: amount.into(),
            refund_address: refund_address.clone(),
        }
        .emit();

        let refund_request = RefundRequest {
            deposit_msg_json: serde_json::to_string(&deposit_msg).unwrap(),
            utxo_storage_key: utxo_storage_key.clone(),
            tx_bytes,
            vout,
            amount,
            refund_address,
            created_at_sec: nano_to_sec(env::block_timestamp()),
        };

        self.data_mut()
            .refund_requests
            .insert(utxo_storage_key, refund_request.into());

        true
    }
}
