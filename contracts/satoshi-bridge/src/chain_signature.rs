use crate::*;
use bitcoin::ecdsa::Signature;
pub const GAS_FOR_SIGN_CALL: Gas = Gas::from_tgas(50);
pub const GAS_FOR_SIGN_BTC_TRANSACTION_CALL_BACK: Gas = Gas::from_tgas(30);

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
        let public_key = self.generate_btc_public_key(
            &self
                .internal_unwrap_btc_pending_info(&btc_pending_sign_id)
                .vutxos[sign_index]
                .get_path(),
        );

        let btc_pending_info = self.internal_unwrap_btc_pending_info(&btc_pending_sign_id);
        require!(
            btc_pending_info.signatures[sign_index].is_none(),
            "Already signed"
        );
        let payload = btc_pending_info
            .get_psbt()
            .get_hash_to_sign(sign_index, &public_key);
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
}

#[near]
impl Contract {
    #[private]
    pub fn sync_root_public_key_callback(&mut self) -> bool {
        if let Some(result_bytes) = promise_result_as_success() {
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
        if let Some(result_bytes) = promise_result_as_success() {
            let signature = serde_json::from_slice::<SignatureResponse>(&result_bytes)
                .expect("Invalid signature");
            let public_key = self
                .generate_btc_public_key(
                    &self
                        .internal_unwrap_btc_pending_info(&btc_pending_sign_id)
                        .vutxos[sign_index]
                        .get_path(),
                )
                .inner;
            let btc_pending_info = self.internal_unwrap_mut_btc_pending_info(&btc_pending_sign_id);
            require!(
                btc_pending_info.signatures[sign_index].is_none(),
                "Already signed"
            );
            btc_pending_info.signatures[sign_index] = Some(signature.clone());
            btc_pending_info.last_sign_time_sec = nano_to_sec(env::block_timestamp());
            Event::BtcInputSignature {
                account_id: &account_id,
                btc_pending_id: &btc_pending_sign_id,
                sign_index,
                signature: &signature,
            }
            .emit();
            let mut psbt = btc_pending_info.get_psbt();
            psbt.save_signature(sign_index, signature, public_key);

            btc_pending_info.psbt_hex = psbt.serialize();
            if btc_pending_info.is_all_signed() {
                let tx_bytes_with_sign = psbt.extract_tx_bytes_with_sign();
                Event::SignedBtcTransaction {
                    account_id: &account_id,
                    tx_id: btc_pending_sign_id.clone(),
                    tx_size: tx_bytes_with_sign.len(),
                }
                .emit();
                btc_pending_info.tx_bytes_with_sign = Some(tx_bytes_with_sign);
                btc_pending_info.to_pending_verify_stage();

                let is_original_tx = btc_pending_info.get_original_tx_id().is_none();
                let account = self.internal_unwrap_mut_account(&account_id);
                let clear_account_btc_pending_sign_id =
                    account.btc_pending_sign_id.take() == Some(btc_pending_sign_id.clone());
                require!(clear_account_btc_pending_sign_id, "Internal error");
                if is_original_tx {
                    account
                        .btc_pending_verify_list
                        .insert(btc_pending_sign_id.clone());
                }
            }
            true
        } else {
            false
        }
    }
}
