use crate::*;

use bitcoin::absolute::LockTime;
use bitcoin::consensus::serialize;
use bitcoin::hashes::Hash;
use bitcoin::psbt::Psbt;
use bitcoin::sighash::SighashCache;
use bitcoin::transaction::Version;
use bitcoin::Transaction as BtcTransaction;
use bitcoin::{OutPoint, TxIn, TxOut, Witness};
use near_sdk::require;

pub struct PsbtWrapper {
    psbt: Psbt,
}
impl PsbtWrapper {
    pub fn new(input: Vec<OutPoint>, output: Vec<TxOut>) -> Self {
        require!(!input.is_empty(), "empty input");
        require!(!output.is_empty(), "empty output");

        let sequence = bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME;

        let transaction = BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: input
                .into_iter()
                .map(|previous_output| TxIn {
                    previous_output,
                    sequence,
                    ..Default::default()
                })
                .collect(),
            output,
        };
        let psbt = Psbt::from_unsigned_tx(transaction).expect("Failed to generate PSBT");

        Self { psbt }
    }

    pub fn from_original_psbt(
        original_psbt: crate::psbt_wrapper::PsbtWrapper,
        output: Vec<TxOut>,
    ) -> Self {
        let sequence = bitcoin::Sequence::MAX;

        let transaction = BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: original_psbt
                .psbt
                .unsigned_tx
                .input
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
        Self { psbt }
    }

    pub fn set_input_utxo(&mut self, input_utxo: Vec<TxOut>) {
        input_utxo
            .iter()
            .enumerate()
            .for_each(|(i, v)| self.psbt.inputs[i].witness_utxo = Some(v.clone()));
    }

    pub fn get_output(&self) -> &Vec<TxOut> {
        &self.psbt.unsigned_tx.output
    }

    pub fn get_input_num(&self) -> usize {
        self.psbt.unsigned_tx.input.len()
    }

    pub fn get_output_num(&self) -> usize {
        self.psbt.unsigned_tx.output.len()
    }

    pub fn get_utxo_storage_keys(&self) -> Vec<String> {
        self.psbt
            .unsigned_tx
            .input
            .clone()
            .into_iter()
            .map(|out_point| {
                generate_utxo_storage_key(
                    out_point.previous_output.txid.to_string(),
                    out_point.previous_output.vout,
                )
            })
            .collect()
    }
    pub fn serialize(&self) -> String {
        self.psbt.serialize_hex()
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let psbt_bytes = hex::decode(psbt_hex).unwrap();
        Self {
            psbt: Psbt::deserialize(&psbt_bytes).expect("ERR_INVALID_PSBT_HEX"),
        }
    }

    pub fn extract_tx_bytes_with_sign(&self) -> Vec<u8> {
        serialize(&self.psbt.clone().extract_tx().expect("extract_tx failed"))
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
        let tx = self.psbt.unsigned_tx.clone();
        let mut cache = SighashCache::new(tx);
        cache
            .p2wpkh_signature_hash(
                vin,
                &self.psbt.inputs[vin]
                    .witness_utxo
                    .as_ref()
                    .unwrap()
                    .script_pubkey,
                self.psbt.inputs[vin].witness_utxo.as_ref().unwrap().value,
                bitcoin::EcdsaSighashType::All,
            )
            .unwrap()
            .to_raw_hash()
            .to_byte_array()
    }

    pub fn save_signature(
        &mut self,
        sign_index: usize,
        signature: SignatureResponse,
        public_key: bitcoin::secp256k1::PublicKey,
    ) {
        self.psbt.inputs[sign_index].final_script_witness =
            Some(Witness::p2wpkh(&signature.to_btc_signature(), &public_key));
    }
}
