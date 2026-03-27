use crate::{
    env, near, require, serde_json, Contract, ContractExt, DepositMsg, Event, Gas, Promise, U128,
};

use crate::deposit_msg::get_deposit_path;
use crate::utils::{generate_utxo_storage_key, nano_to_sec};

pub const GAS_FOR_REQUEST_REFUND_CALLBACK: Gas = Gas::from_tgas(20);

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct RefundRequest {
    pub deposit_msg: DepositMsg,
    pub utxo_storage_key: String,
    pub tx_bytes: Vec<u8>,
    pub vout: usize,
    pub amount: u128,
    pub created_at_sec: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
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

        let tx_id = crate::utils::compute_tx_id(&tx_bytes);
        let utxo_storage_key = generate_utxo_storage_key(tx_id.clone(), vout as u32);

        // Must not be already verified/finalized
        require!(
            !self.data().verified_deposit_utxo.contains(&utxo_storage_key),
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
    /// Creates a BTC pending info for signing and broadcasting.
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
            now >= refund_request.created_at_sec + config.refund_timelock_sec,
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

        let refund_address = refund_request
            .deposit_msg
            .refund_address
            .clone()
            .expect("No refund address");

        // TODO: Build PSBT with:
        //   input:  the deposit UTXO (tx_bytes + vout)
        //   output: refund_address (amount - gas_fee)
        // Then create BTCPendingInfo for sign pipeline.
        //
        // The actual PSBT construction will be chain-specific (bitcoin_utils).

        Event::RefundExecuted {
            utxo_storage_key: utxo_storage_key.clone(),
            amount: refund_request.amount.into(),
            refund_address,
        }
        .emit();

        self.data_mut().refund_requests.remove(&utxo_storage_key);

        // Do NOT add to verified_deposit_utxo — the UTXO will be spent on Bitcoin,
        // which prevents double-spend naturally.
    }
}

#[near]
impl Contract {
    #[private]
    pub fn request_refund_callback(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: usize,
    ) -> bool {
        let result_bytes = crate::promise_result_as_success()
            .expect("Call verify_transaction_inclusion failed");
        let is_valid = serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion return not bool");
        require!(is_valid, "verify_transaction_inclusion return false");

        // Extract amount from tx output
        let config = self.internal_config();
        let transaction = crate::WrappedTransaction::decode(&tx_bytes, &config.chain)
            .expect("Deserialization tx_bytes failed");
        let output = &transaction.output()[vout];

        // Verify that the output script matches the deposit address derived from deposit_msg
        let path = get_deposit_path(&deposit_msg);
        let deposit_script_pubkey = config.get_deposit_script_pubkey(&path);
        require!(
            deposit_script_pubkey == output.script_pubkey,
            "Output script_pubkey does not match deposit address"
        );

        let amount = output.value.to_sat() as u128;
        let tx_id = crate::utils::compute_tx_id(&tx_bytes);
        let utxo_storage_key = generate_utxo_storage_key(tx_id, vout as u32);

        // Double-check not finalized (could have been verified between request and callback)
        require!(
            !self.data().verified_deposit_utxo.contains(&utxo_storage_key),
            "UTXO already verified via deposit"
        );

        let refund_address = deposit_msg
            .refund_address
            .clone()
            .expect("No refund address");

        Event::RefundRequested {
            utxo_storage_key: utxo_storage_key.clone(),
            amount: amount.into(),
            refund_address,
        }
        .emit();

        let refund_request = RefundRequest {
            deposit_msg,
            utxo_storage_key: utxo_storage_key.clone(),
            tx_bytes,
            vout,
            amount,
            created_at_sec: nano_to_sec(env::block_timestamp()),
        };

        self.data_mut()
            .refund_requests
            .insert(utxo_storage_key, refund_request.into());

        true
    }
}
