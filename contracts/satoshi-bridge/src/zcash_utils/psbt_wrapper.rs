use crate::*;

use bitcoin::absolute::LockTime;
use bitcoin::hashes::Hash;
use bitcoin::psbt::Psbt;
use bitcoin::transaction::Version;
use bitcoin::{OutPoint, TxIn, TxOut};
use near_sdk::require;
use zcash_primitives::transaction::fees::transparent::{InputSize, OutputView};
use zcash_primitives::transaction::fees::FeeRule;
use zcash_protocol::consensus::BlockHeight;
use zcash_protocol::value::Zatoshis;
use zcash_transparent::bundle::Authorized;

pub struct PsbtWrapper {
    psbt: Psbt,
    expiry_height: u32,
    pub vin: Vec<zcash_transparent::bundle::TxIn<Authorized>>,
    pub vout: Vec<zcash_transparent::bundle::TxOut>,
    pub inputs: Vec<zcash_transparent::bundle::TxOut>,
}

impl PsbtWrapper {
    pub fn new(input: Vec<OutPoint>, output: Vec<TxOut>, expiry_height: u32) -> Self {
        require!(!input.is_empty(), "empty input");
        require!(!output.is_empty(), "empty output");

        let sequence = bitcoin::Sequence::MAX;

        let transaction = BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: input
                .clone()
                .into_iter()
                .map(|previous_output| TxIn {
                    previous_output,
                    sequence,
                    ..Default::default()
                })
                .collect(),
            output: output.clone(),
        };
        let psbt = Psbt::from_unsigned_tx(transaction).expect("Failed to generate PSBT");

        let vout = output
            .clone()
            .into_iter()
            .map(|o| zcash_transparent::bundle::TxOut {
                value: Zatoshis::from_u64(o.value.to_sat()).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script(o.script_pubkey.to_bytes()),
            })
            .collect();

        let vin: Vec<zcash_transparent::bundle::TxIn<Authorized>> = psbt
            .clone()
            .unsigned_tx
            .input
            .into_iter()
            .map(|i| zcash_transparent::bundle::TxIn {
                prevout: zcash_transparent::bundle::OutPoint::new(
                    i.previous_output.txid.to_byte_array(),
                    i.previous_output.vout,
                ),
                script_sig: zcash_primitives::legacy::Script::default(),
                sequence: sequence.0,
            })
            .collect();

        let inputs = vec![
            zcash_transparent::bundle::TxOut {
                value: Zatoshis::from_u64(0).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script::default(),
            };
            vin.len()
        ];

        Self {
            psbt,
            expiry_height,
            vout,
            vin,
            inputs,
        }
    }

    pub fn from_original_psbt(
        original_psbt: PsbtWrapper,
        output: Vec<TxOut>,
        expiry_height: u32,
    ) -> Self {
        let sequence = bitcoin::Sequence::MAX;
        let mut output = output;
        if output.is_empty() {
            output = original_psbt
                .get_output()
                .into_iter()
                .map(|original_psbt_output| TxOut {
                    value: original_psbt_output.value,
                    script_pubkey: original_psbt_output.script_pubkey.clone(),
                })
                .collect()
        }

        let vout = output
            .clone()
            .into_iter()
            .map(|o| zcash_transparent::bundle::TxOut {
                value: Zatoshis::from_u64(o.value.to_sat()).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script(o.script_pubkey.to_bytes()),
            })
            .collect();

        let transaction = BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: original_psbt
                .get_input()
                .into_iter()
                .map(|original_psbt_input| TxIn {
                    previous_output: original_psbt_input.previous_output,
                    sequence,
                    ..Default::default()
                })
                .collect(),
            output,
        };
        let mut psbt = Psbt::from_unsigned_tx(transaction).expect("Failed to generate PSBT");
        original_psbt
            .psbt
            .inputs
            .iter()
            .enumerate()
            .for_each(|(i, v)| {
                psbt.inputs[i].witness_utxo.clone_from(&v.witness_utxo);
            });

        Self {
            psbt,
            expiry_height,
            vin: original_psbt.vin,
            vout,
            inputs: original_psbt.inputs,
        }
    }

    pub fn set_input_utxo(&mut self, input_utxo: Vec<TxOut>) {
        input_utxo
            .iter()
            .enumerate()
            .for_each(|(i, v)| self.psbt.inputs[i].witness_utxo = Some(v.clone()));

        input_utxo.iter().enumerate().for_each(|(i, v)| {
            self.inputs[i] = zcash_transparent::bundle::TxOut {
                value: Zatoshis::from_u64(v.value.to_sat()).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script(v.script_pubkey.to_bytes()),
            }
        });
    }

    pub fn get_input(&self) -> &Vec<TxIn> {
        &self.psbt.unsigned_tx.input
    }

    pub fn get_output(&self) -> &Vec<TxOut> {
        &self.psbt.unsigned_tx.output
    }

    pub fn serialize(&self) -> String {
        let h_encode = hex::encode(self.expiry_height.to_le_bytes());
        format!("{}{}", h_encode, self.psbt.serialize_hex())
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let h_hex = hex::decode(&psbt_hex[..8]).unwrap();
        let psbt_bytes = hex::decode(&psbt_hex[8..]).unwrap();

        let psbt = Psbt::deserialize(&psbt_bytes).expect("ERR_INVALID_PSBT_HEX");

        let vout = psbt
            .clone()
            .unsigned_tx
            .output
            .clone()
            .into_iter()
            .map(|o| zcash_transparent::bundle::TxOut {
                value: Zatoshis::from_u64(o.value.to_sat()).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script(o.script_pubkey.to_bytes()),
            })
            .collect();

        let vin: Vec<zcash_transparent::bundle::TxIn<Authorized>> = psbt
            .clone()
            .unsigned_tx
            .input
            .into_iter()
            .map(|i| zcash_transparent::bundle::TxIn {
                prevout: zcash_transparent::bundle::OutPoint::new(
                    i.previous_output.txid.to_byte_array(),
                    i.previous_output.vout,
                ),
                script_sig: zcash_primitives::legacy::Script(i.script_sig.to_bytes()),
                sequence: i.sequence.0,
            })
            .collect();

        let inputs = psbt
            .clone()
            .inputs
            .into_iter()
            .map(|i| zcash_transparent::bundle::TxOut {
                value: Zatoshis::from_u64(i.witness_utxo.clone().unwrap().value.to_sat()).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script(
                    i.witness_utxo.clone().unwrap().script_pubkey.to_bytes(),
                ),
            })
            .collect();

        Self {
            psbt,
            expiry_height: u32::from_le_bytes(h_hex.try_into().unwrap()),
            vin,
            vout,
            inputs,
        }
    }

    pub fn extract_tx_bytes_with_sign(&self) -> Vec<u8> {
        let transaction = self.psbt.clone().extract_tx().expect("extract_tx failed");
        WrappedTransaction::tx_bytes_with_sign(transaction, self.expiry_height).unwrap()
    }

    pub fn get_pending_id(&self) -> String {
        self.psbt
            .clone()
            .extract_tx()
            .unwrap()
            .compute_txid()
            .to_string()
    }

    #[allow(unused_variables)]
    pub fn get_hash_to_sign(&self, vin: usize, public_key: &bitcoin::PublicKey) -> [u8; 32] {
        use zcash_protocol::value::Zatoshis;
        use zcash_transparent::sighash::SighashType;

        let tx_data = WrappedTransaction::to_zcash_tx(&self.psbt, public_key, self.expiry_height);
        let txid_parts = tx_data.digest(zcash_primitives::transaction::txid::TxIdDigester);

        let script_pubkey = &self.psbt.inputs[vin]
            .witness_utxo
            .as_ref()
            .unwrap()
            .script_pubkey;

        let value: u64 = self.psbt.inputs[vin]
            .witness_utxo
            .as_ref()
            .unwrap()
            .value
            .to_sat();

        let script = zcash_primitives::legacy::Script(script_pubkey.clone().into_bytes());

        let sig_input = zcash_primitives::transaction::sighash::SignableInput::Transparent(
            zcash_transparent::sighash::SignableInput::from_parts(
                SighashType::ALL,
                vin,
                &script,
                &script,
                Zatoshis::from_u64(value).unwrap(),
            ),
        );

        zcash_primitives::transaction::sighash::signature_hash(&tx_data, &sig_input, &txid_parts)
            .as_ref()
            .clone()
    }

    pub fn save_signature(
        &mut self,
        sign_index: usize,
        signature: SignatureResponse,
        public_key: bitcoin::secp256k1::PublicKey,
    ) {
        let script_sig = bitcoin::script::Builder::new()
            .push_slice(signature.to_btc_signature().serialize())
            .push_key(&bitcoin::PublicKey::new(public_key))
            .into_script();

        self.psbt.inputs[sign_index].final_script_sig = Some(script_sig);
    }

    pub fn get_min_fee(&self) -> Zatoshis {
        let fee_rule = zcash_primitives::transaction::fees::zip317::FeeRule::standard();
        fee_rule
            .fee_required(
                &zcash_protocol::consensus::MainNetwork,
                BlockHeight::from_u32(0u32),
                vec![InputSize::STANDARD_P2PKH; self.vin.len()],
                self.vout.iter().map(|i| i.serialized_size()),
                0,
                0,
                0,
            )
            .unwrap()
    }
}
