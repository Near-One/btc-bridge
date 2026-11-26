use crate::{generate_utxo_storage_key, SignatureResponse};

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
        let sequence = bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME;

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

    pub fn add_extra_outputs(&self, _actual_received_amounts: &mut [u128]) -> u128 {
        0
    }
    pub fn serialize(&self) -> String {
        self.psbt.serialize_hex()
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let psbt_bytes = hex::decode(psbt_hex).expect("ERR_INVALID_PSBT_HEX: failed to decode hex");
        Self {
            psbt: Psbt::deserialize(&psbt_bytes).expect("ERR_INVALID_PSBT: failed to parse PSBT"),
        }
    }

    pub fn extract_tx_bytes_with_sign(&self) -> Vec<u8> {
        serialize(&self.psbt.clone().extract_tx().expect("extract_tx failed"))
    }

    pub fn get_expiry_height(&self) -> Option<u32> {
        None
    }

    pub fn get_pending_id(&self) -> String {
        self.psbt
            .clone()
            .extract_tx()
            .expect("ERR_EXTRACT_TX: failed to extract transaction from PSBT")
            .compute_txid()
            .to_string()
    }

    pub fn get_hash_to_sign(&self, vin: usize) -> [u8; 32] {
        self.get_hashes_to_sign(&[vin])[0]
    }

    /// BIP143 sighashes of the given inputs, in the same order. One
    /// `SighashCache` is shared by all inputs, so hashPrevouts, hashSequence
    /// and hashOutputs are computed once.
    pub fn get_hashes_to_sign(&self, vins: &[usize]) -> Vec<[u8; 32]> {
        let mut cache = SighashCache::new(&self.psbt.unsigned_tx);
        vins.iter()
            .map(|&vin| {
                let witness_utxo = self.psbt.inputs[vin]
                    .witness_utxo
                    .as_ref()
                    .expect("ERR_MISSING_WITNESS_UTXO: input missing witness UTXO data");
                cache
                    .p2wpkh_signature_hash(
                        vin,
                        &witness_utxo.script_pubkey,
                        witness_utxo.value,
                        bitcoin::EcdsaSighashType::All,
                    )
                    .expect("ERR_SIGHASH: failed to compute signature hash")
                    .to_raw_hash()
                    .to_byte_array()
            })
            .collect()
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

    pub fn get_recipient_address(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{Amount, ScriptBuf, Txid, WPubkeyHash};

    fn p2wpkh(seed: u8) -> ScriptBuf {
        ScriptBuf::new_p2wpkh(&WPubkeyHash::from_byte_array([seed; 20]))
    }

    /// The previous implementation: a fresh `SighashCache` per input.
    fn hash_with_fresh_cache(psbt: &Psbt, vin: usize) -> [u8; 32] {
        let utxo = psbt.inputs[vin].witness_utxo.as_ref().unwrap();
        SighashCache::new(psbt.unsigned_tx.clone())
            .p2wpkh_signature_hash(
                vin,
                &utxo.script_pubkey,
                utxo.value,
                bitcoin::EcdsaSighashType::All,
            )
            .unwrap()
            .to_raw_hash()
            .to_byte_array()
    }

    #[test]
    fn test_shared_cache_sighashes_match_per_input_sighashes() {
        let input_num = 7;
        let inputs = (0..input_num)
            .map(|i| OutPoint {
                txid: Txid::from_byte_array([i as u8 + 1; 32]),
                vout: i as u32,
            })
            .collect();
        let outputs = vec![
            TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: p2wpkh(0xaa),
            },
            TxOut {
                value: Amount::from_sat(20_000),
                script_pubkey: p2wpkh(0xbb),
            },
        ];
        let mut psbt = PsbtWrapper::new(inputs, outputs);
        psbt.set_input_utxo(
            (0..input_num)
                .map(|i| TxOut {
                    value: Amount::from_sat(10_000 + i as u64),
                    script_pubkey: p2wpkh(i as u8),
                })
                .collect(),
        );

        let expected: Vec<[u8; 32]> = (0..input_num)
            .map(|vin| hash_with_fresh_cache(&psbt.psbt, vin))
            .collect();

        let all: Vec<usize> = (0..input_num).collect();
        assert_eq!(psbt.get_hashes_to_sign(&all), expected);
        // Any subset, in any order, keeps the requested order.
        assert_eq!(
            psbt.get_hashes_to_sign(&[5, 1, 3]),
            vec![expected[5], expected[1], expected[3]]
        );
        for (vin, hash) in expected.iter().enumerate() {
            assert_eq!(&psbt.get_hash_to_sign(vin), hash);
        }
        // Different inputs give different sighashes.
        assert_ne!(expected[0], expected[1]);
    }
}
