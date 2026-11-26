use crate::{
    env, ext_contract, nano_to_sec, near, require, serde_json, AccountId, Contract, ContractExt,
    Event, Gas, HashMap, Promise, PublicKey, MAX_PUBLIC_KEY_RESULT, MAX_SIGNATURE_RESULT,
};
use bitcoin::ecdsa::Signature;

pub const GAS_FOR_SIGN_CALL: Gas = Gas::from_tgas(50);
pub const GAS_FOR_SIGN_BTC_TRANSACTION_CALL_BACK: Gas = Gas::from_tgas(30);

/// Static gas of each MPC `sign` call made by `sign_btc_transaction_batch`.
/// The MPC contract requires at least 15 Tgas prepaid. `sign` burns about
/// 4.6 Tgas and reserves 7 Tgas for its resume callback.
pub const GAS_FOR_BATCH_SIGN_CALL: Gas = Gas::from_tgas(20);
/// Static gas of `sign_btc_transaction_batch_callback`, fixed part. Measured
/// about 4 Tgas, plus about 14 Tgas when the PSBT carries an Orchard bundle.
pub const GAS_FOR_BATCH_SIGN_CALLBACK_BASE: Gas = Gas::from_tgas(40);
/// Static gas of `sign_btc_transaction_batch_callback`, part per signed input.
/// Measured about 5 Tgas, almost all of it the public key derivation.
pub const GAS_FOR_BATCH_SIGN_CALLBACK_PER_INPUT: Gas = Gas::from_tgas(6);

#[near(serializers = [borsh, json])]
pub struct SignRequest {
    pub payload: [u8; 32],
    pub path: String,
    pub key_version: u32,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct BigR {
    pub affine_point: String,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct S {
    pub scalar: String,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct SignatureResponse {
    pub big_r: BigR,
    pub s: S,
    pub recovery_id: u8,
}

impl SignatureResponse {
    pub fn to_btc_signature(&self) -> Signature {
        let r_hex = self.big_r.affine_point[2..].to_string();
        let s_hex = self.s.scalar.clone();
        let r = hex::decode(r_hex).expect("Invalid r hex");
        let s = hex::decode(s_hex).expect("Invalid s hex");
        let signature = bitcoin::secp256k1::ecdsa::Signature::from_compact(&[r, s].concat())
            .expect("Invalid signature");
        Signature::sighash_all(signature)
    }
}

#[near(serializers=[json])]
pub struct DomainId(pub u64);

#[ext_contract(ext_chain_signatures)]
pub trait ChainSignatures {
    fn sign(&mut self, request: SignRequest) -> Promise;
    fn public_key(&self, domain_id: Option<DomainId>) -> PublicKey;
}

impl Contract {
    pub fn sign_promise(&self, request: SignRequest) -> Promise {
        let config = self.internal_config();
        ext_chain_signatures::ext(config.chain_signatures_account_id.clone())
            .with_static_gas(GAS_FOR_SIGN_CALL)
            .with_attached_deposit(env::attached_deposit())
            .sign(request)
    }

    pub fn sync_chain_signatures_root_public_key_promise(&mut self) -> Promise {
        ext_chain_signatures::ext(self.internal_config().chain_signatures_account_id.clone())
            .public_key(None)
            .then(Self::ext(env::current_account_id()).sync_root_public_key_callback())
    }

    pub fn internal_sign_btc_transaction(
        &mut self,
        btc_pending_sign_id: String,
        sign_index: usize,
        key_version: u32,
    ) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&btc_pending_sign_id);
        require!(
            btc_pending_info.signatures[sign_index].is_none(),
            "Already signed"
        );
        let payload = btc_pending_info.get_psbt().get_hash_to_sign(sign_index);
        let path = btc_pending_info.vutxos[sign_index].get_path();
        self.sign_promise(SignRequest {
            payload,
            path,
            key_version,
        })
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_SIGN_BTC_TRANSACTION_CALL_BACK)
                .sign_btc_transaction_callback(
                    btc_pending_info.account_id.clone(),
                    btc_pending_sign_id,
                    sign_index,
                ),
        )
    }

    /// Checks shared by `sign_btc_transaction` and `sign_btc_transaction_batch`.
    ///
    /// Returns `false` if the pending info is an RBF whose original transaction
    /// no longer exists. In that case the pending info has been removed and
    /// there is nothing to sign.
    pub fn internal_prepare_sign_btc_transaction(&mut self, btc_pending_sign_id: &String) -> bool {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(btc_pending_sign_id);
        require!(
            btc_pending_info.is_refund() || self.is_trusted_relayer(&env::predecessor_account_id()),
            "Relayer is not active"
        );
        btc_pending_info.assert_pending_sign();
        if let Some(original_tx_id) = btc_pending_info.get_original_tx_id() {
            if !self.check_btc_pending_info_exists(original_tx_id) {
                let account_id = btc_pending_info.account_id.clone();
                require!(
                    self.internal_unwrap_mut_account(&account_id)
                        .btc_pending_sign_ids
                        .remove(btc_pending_sign_id),
                    "Internal error"
                );
                self.internal_remove_btc_pending_info(btc_pending_sign_id);
                return false;
            }
        }
        true
    }

    /// Request MPC signatures for several inputs in one call.
    ///
    /// Indexes that are already signed are skipped, so a relayer can retry
    /// with the same list. Each input gets its own `sign` promise: MPC nodes
    /// ignore receipts with more than one action. The `sign` promises are
    /// joined into one `sign_btc_transaction_batch_callback`, which stores all
    /// signatures with one PSBT parse and one PSBT write.
    ///
    /// The attached deposit is split evenly between the `sign` calls (the
    /// first one also gets the remainder), so it must be at least one
    /// yoctoNEAR per input to sign.
    pub fn internal_sign_btc_transaction_batch(
        &mut self,
        btc_pending_sign_id: String,
        mut sign_indexes: Vec<usize>,
        key_version: u32,
    ) -> Promise {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&btc_pending_sign_id);
        sign_indexes.sort_unstable();
        sign_indexes.dedup();
        require!(
            sign_indexes
                .last()
                .is_none_or(|&i| i < btc_pending_info.signatures.len()),
            "Invalid sign index"
        );
        sign_indexes.retain(|&i| btc_pending_info.signatures[i].is_none());
        require!(!sign_indexes.is_empty(), "Already signed");

        let sign_num = sign_indexes.len();
        let attached_deposit = env::attached_deposit();
        let deposit_per_sign = attached_deposit.saturating_div(sign_num as u128);
        require!(
            !deposit_per_sign.is_zero(),
            "Attach at least 1 yoctoNEAR per input to sign"
        );
        // The first `sign` call also gets the remainder of the division, so
        // the whole deposit goes to MPC, as in `sign_btc_transaction`. MPC
        // refunds any deposit above its fee to this contract.
        let first_sign_deposit = attached_deposit
            .saturating_sub(deposit_per_sign.saturating_mul((sign_num - 1) as u128));

        let payloads = btc_pending_info
            .get_psbt()
            .get_hashes_to_sign(&sign_indexes);
        let chain_signatures_account_id =
            self.internal_config().chain_signatures_account_id.clone();
        let sign_promises = sign_indexes
            .iter()
            .zip(payloads)
            .enumerate()
            .map(|(i, (&sign_index, payload))| {
                ext_chain_signatures::ext(chain_signatures_account_id.clone())
                    .with_static_gas(GAS_FOR_BATCH_SIGN_CALL)
                    .with_unused_gas_weight(0)
                    .with_attached_deposit(if i == 0 {
                        first_sign_deposit
                    } else {
                        deposit_per_sign
                    })
                    .sign(SignRequest {
                        payload,
                        path: btc_pending_info.vutxos[sign_index].get_path(),
                        key_version,
                    })
            })
            .reduce(Promise::and)
            .unwrap_or_else(|| env::panic_str("Already signed"));

        // The `sign` calls get a fixed amount of gas. The callback gets its
        // static gas plus all gas left over, so the relayer can attach more
        // if a transaction needs it.
        sign_promises.then(
            Self::ext(env::current_account_id())
                .with_static_gas(batch_sign_callback_gas(sign_num))
                .sign_btc_transaction_batch_callback(
                    btc_pending_info.account_id.clone(),
                    btc_pending_sign_id,
                    sign_indexes,
                ),
        )
    }

    /// Store MPC signatures for inputs of a pending transaction.
    ///
    /// The caller must make sure every index is unique and not signed yet.
    /// The PSBT is parsed and written back once. When the last input is
    /// signed, the signed transaction is built and the pending info moves to
    /// the pending verify stage.
    ///
    /// Events: one `btc_input_signature` per stored signature, and
    /// `signed_btc_transaction` when the transaction is complete. For a batch
    /// that completes the transaction, only `signed_btc_transaction` is
    /// emitted: its witnesses carry every signature, and with one
    /// `btc_input_signature` per input the receipt would exceed the protocol
    /// limit on total log length (16 KiB) at about 15 inputs.
    fn internal_save_signatures(
        &mut self,
        account_id: &AccountId,
        btc_pending_sign_id: &String,
        signatures: Vec<(usize, SignatureResponse)>,
        is_batch: bool,
    ) {
        // Inputs often share a derivation path (e.g. the change address), and
        // the key derivation is the costly part, so derive each path once.
        let mut public_keys: HashMap<String, bitcoin::secp256k1::PublicKey> = HashMap::new();
        let signatures: Vec<_> = {
            let btc_pending_info = self.internal_unwrap_btc_pending_info(btc_pending_sign_id);
            signatures
                .into_iter()
                .map(|(sign_index, signature)| {
                    let path = btc_pending_info.vutxos[sign_index].get_path();
                    let public_key = *public_keys
                        .entry(path)
                        .or_insert_with_key(|path| self.generate_btc_public_key(path).inner);
                    (sign_index, signature, public_key)
                })
                .collect()
        };

        let btc_pending_info = self.internal_unwrap_mut_btc_pending_info(btc_pending_sign_id);
        let mut psbt = btc_pending_info.get_psbt();
        for (sign_index, signature, public_key) in &signatures {
            btc_pending_info.signatures[*sign_index] = Some(signature.clone());
            psbt.save_signature(*sign_index, signature.clone(), *public_key);
        }
        btc_pending_info.last_sign_time_sec = nano_to_sec(env::block_timestamp());
        btc_pending_info.psbt_hex = psbt.serialize();
        let is_all_signed = btc_pending_info.is_all_signed();
        if !(is_batch && is_all_signed) {
            for (sign_index, signature, _) in &signatures {
                Event::BtcInputSignature {
                    account_id,
                    btc_pending_id: btc_pending_sign_id,
                    sign_index: *sign_index,
                    signature,
                }
                .emit();
            }
        }

        if is_all_signed {
            let tx_bytes_with_sign = psbt.extract_tx_bytes_with_sign();

            // For ZCash chains, use base64 encoding to save space (1.33x vs 2x overhead for hex)
            // ZCash transactions with Orchard bundles are larger and benefit from compact encoding
            // For Bitcoin chains, keep hex encoding for backward compatibility

            #[cfg(feature = "zcash")]
            let tx_bytes_base64 = {
                use near_sdk::base64::{engine::general_purpose::STANDARD, Engine};
                STANDARD.encode(&tx_bytes_with_sign)
            };

            Event::SignedBtcTransaction {
                account_id,
                tx_id: btc_pending_sign_id.clone(),
                #[cfg(not(feature = "zcash"))]
                tx_bytes: &tx_bytes_with_sign,
                #[cfg(feature = "zcash")]
                tx_bytes_base64,
            }
            .emit();

            btc_pending_info.tx_bytes_with_sign = Some(tx_bytes_with_sign);
            btc_pending_info.to_pending_verify_stage();

            let is_original_tx = btc_pending_info.get_original_tx_id().is_none();
            let account = self.internal_unwrap_mut_account(account_id);
            require!(
                account.btc_pending_sign_ids.remove(btc_pending_sign_id),
                "Internal error"
            );
            if is_original_tx {
                account
                    .btc_pending_verify_list
                    .insert(btc_pending_sign_id.clone());
            }
        }
    }
}

/// Static gas for a batch callback that stores `sign_num` signatures.
pub fn batch_sign_callback_gas(sign_num: usize) -> Gas {
    GAS_FOR_BATCH_SIGN_CALLBACK_PER_INPUT
        .saturating_mul(sign_num as u64)
        .saturating_add(GAS_FOR_BATCH_SIGN_CALLBACK_BASE)
}

#[near]
impl Contract {
    #[private]
    pub fn sync_root_public_key_callback(&mut self) -> bool {
        if let Ok(result_bytes) = env::promise_result_checked(0, MAX_PUBLIC_KEY_RESULT) {
            let root_public_key =
                serde_json::from_slice::<PublicKey>(&result_bytes).expect("Invalid PublicKey");
            self.internal_mut_config().chain_signatures_root_public_key = Some(root_public_key);
            let change_address = self
                .generate_utxo_chain_address(env::current_account_id().as_str())
                .to_string();
            self.internal_mut_config().change_address = Some(change_address);
            true
        } else {
            false
        }
    }

    #[private]
    pub fn sign_btc_transaction_callback(
        &mut self,
        account_id: AccountId,
        btc_pending_sign_id: String,
        sign_index: usize,
    ) -> bool {
        if let Ok(result_bytes) = env::promise_result_checked(0, MAX_SIGNATURE_RESULT) {
            let signature = serde_json::from_slice::<SignatureResponse>(&result_bytes)
                .expect("Invalid signature");
            require!(
                self.internal_unwrap_btc_pending_info(&btc_pending_sign_id)
                    .signatures[sign_index]
                    .is_none(),
                "Already signed"
            );
            self.internal_save_signatures(
                &account_id,
                &btc_pending_sign_id,
                vec![(sign_index, signature)],
                false,
            );
            true
        } else {
            false
        }
    }

    /// Callback of `sign_btc_transaction_batch`. Promise result `i` is the MPC
    /// response for `sign_indexes[i]`.
    ///
    /// A failed or unparsable response, or an input that another call signed
    /// in the meantime, is skipped without affecting the other inputs; the
    /// relayer can request it again. Returns `true` if every signature was
    /// stored.
    #[private]
    pub fn sign_btc_transaction_batch_callback(
        &mut self,
        #[serializer(borsh)] account_id: AccountId,
        #[serializer(borsh)] btc_pending_sign_id: String,
        #[serializer(borsh)] sign_indexes: Vec<usize>,
    ) -> bool {
        require!(
            env::promise_results_count() == sign_indexes.len() as u64,
            "Promise results count mismatch"
        );
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&btc_pending_sign_id);
        let signatures: Vec<(usize, SignatureResponse)> = sign_indexes
            .iter()
            .enumerate()
            .filter(|(_, &sign_index)| btc_pending_info.signatures[sign_index].is_none())
            .filter_map(|(result_index, &sign_index)| {
                env::promise_result_checked(result_index as u64, MAX_SIGNATURE_RESULT)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<SignatureResponse>(&bytes).ok())
                    .map(|signature| (sign_index, signature))
            })
            .collect();
        let all_stored = signatures.len() == sign_indexes.len();
        if !signatures.is_empty() {
            self.internal_save_signatures(&account_id, &btc_pending_sign_id, signatures, true);
        }
        all_stored
    }
}
