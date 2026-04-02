use crate::{generate_utxo_storage_key, SignatureResponse};

use bitcoin::absolute::LockTime;
use bitcoin::consensus::serialize;
use bitcoin::hashes::Hash;
use bitcoin::sighash::SighashCache;
use bitcoin::transaction::Version;
use bitcoin::Transaction as BtcTransaction;
use bitcoin::{OutPoint, ScriptBuf, TxIn, TxOut, Witness};
use near_sdk::require;

const PSBT_FORMAT_VERSION: u8 = 1;
const MAX_TX_BYTES: usize = 1_000_000;
const MAX_ITEM_COUNT: usize = 1_000;
const MAX_ELEMENT_BYTES: usize = 100_000;

pub struct PsbtWrapper {
    unsigned_tx: BtcTransaction,
    input_utxos: Vec<Option<TxOut>>,
    final_script_sigs: Vec<Option<ScriptBuf>>,
}

impl PsbtWrapper {
    pub fn new(input: Vec<OutPoint>, output: Vec<TxOut>) -> Self {
        require!(!input.is_empty(), "empty input");
        require!(!output.is_empty(), "empty output");

        let sequence = bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME;
        let input_count = input.len();

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

        Self {
            unsigned_tx: transaction,
            input_utxos: vec![None; input_count],
            final_script_sigs: vec![None; input_count],
        }
    }

    pub fn from_original_psbt(
        original_psbt: crate::psbt_wrapper::PsbtWrapper,
        output: Vec<TxOut>,
    ) -> Self {
        let sequence = bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME;
        let input_count = original_psbt.unsigned_tx.input.len();

        let transaction = BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: original_psbt
                .unsigned_tx
                .input
                .into_iter()
                .map(|original_input| TxIn {
                    previous_output: original_input.previous_output,
                    sequence,
                    ..Default::default()
                })
                .collect(),
            output,
        };

        Self {
            unsigned_tx: transaction,
            input_utxos: original_psbt.input_utxos,
            final_script_sigs: vec![None; input_count],
        }
    }

    pub fn set_input_utxo(&mut self, input_utxo: Vec<TxOut>) {
        input_utxo
            .iter()
            .enumerate()
            .for_each(|(i, v)| self.input_utxos[i] = Some(v.clone()));
    }

    pub fn get_output(&self) -> &Vec<TxOut> {
        &self.unsigned_tx.output
    }

    pub fn get_input_num(&self) -> usize {
        self.unsigned_tx.input.len()
    }

    pub fn get_output_num(&self) -> usize {
        self.unsigned_tx.output.len()
    }

    pub fn get_utxo_storage_keys(&self) -> Vec<String> {
        self.unsigned_tx
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
        let mut buf = Vec::<u8>::new();
        buf.push(PSBT_FORMAT_VERSION);

        let tx_bytes = serialize(&self.unsigned_tx);
        buf.extend_from_slice(&(tx_bytes.len() as u64).to_le_bytes());
        buf.extend_from_slice(&tx_bytes);

        buf.extend_from_slice(&(self.input_utxos.len() as u64).to_le_bytes());
        for utxo in &self.input_utxos {
            match utxo {
                Some(txout) => {
                    buf.push(1);
                    let utxo_bytes = serialize(txout);
                    buf.extend_from_slice(&(utxo_bytes.len() as u64).to_le_bytes());
                    buf.extend_from_slice(&utxo_bytes);
                }
                None => buf.push(0),
            }
        }

        buf.extend_from_slice(&(self.final_script_sigs.len() as u64).to_le_bytes());
        for sig in &self.final_script_sigs {
            match sig {
                Some(script) => {
                    buf.push(1);
                    let script_bytes = script.as_bytes();
                    buf.extend_from_slice(&(script_bytes.len() as u64).to_le_bytes());
                    buf.extend_from_slice(script_bytes);
                }
                None => buf.push(0),
            }
        }

        hex::encode(buf)
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let bytes = hex::decode(psbt_hex).expect("ERR_INVALID_PSBT_HEX");
        let mut cursor = std::io::Cursor::new(&bytes);

        use std::io::Read;

        let mut version_buf = [0u8; 1];
        cursor
            .read_exact(&mut version_buf)
            .expect("ERR_INVALID_PSBT");
        require!(
            version_buf[0] == PSBT_FORMAT_VERSION,
            "ERR_INVALID_PSBT: unsupported version"
        );

        let mut tx_len_buf = [0u8; 8];
        cursor
            .read_exact(&mut tx_len_buf)
            .expect("ERR_INVALID_PSBT");
        let tx_len = u64::from_le_bytes(tx_len_buf) as usize;
        require!(tx_len <= MAX_TX_BYTES, "ERR_INVALID_PSBT: tx too large");
        let mut tx_bytes = vec![0u8; tx_len];
        cursor.read_exact(&mut tx_bytes).expect("ERR_INVALID_PSBT");
        let unsigned_tx: BtcTransaction =
            bitcoin::consensus::deserialize(&tx_bytes).expect("ERR_INVALID_PSBT");

        let mut utxo_count_buf = [0u8; 8];
        cursor
            .read_exact(&mut utxo_count_buf)
            .expect("ERR_INVALID_PSBT");
        let utxo_count = u64::from_le_bytes(utxo_count_buf) as usize;
        require!(
            utxo_count <= MAX_ITEM_COUNT,
            "ERR_INVALID_PSBT: too many utxos"
        );
        let mut input_utxos = Vec::with_capacity(utxo_count);
        for _ in 0..utxo_count {
            let mut flag = [0u8; 1];
            cursor.read_exact(&mut flag).expect("ERR_INVALID_PSBT");
            if flag[0] == 1 {
                let mut utxo_len_buf = [0u8; 8];
                cursor
                    .read_exact(&mut utxo_len_buf)
                    .expect("ERR_INVALID_PSBT");
                let utxo_len = u64::from_le_bytes(utxo_len_buf) as usize;
                require!(
                    utxo_len <= MAX_ELEMENT_BYTES,
                    "ERR_INVALID_PSBT: utxo too large"
                );
                let mut utxo_bytes = vec![0u8; utxo_len];
                cursor
                    .read_exact(&mut utxo_bytes)
                    .expect("ERR_INVALID_PSBT");
                let txout: TxOut =
                    bitcoin::consensus::deserialize(&utxo_bytes).expect("ERR_INVALID_PSBT");
                input_utxos.push(Some(txout));
            } else {
                input_utxos.push(None);
            }
        }

        let mut sig_count_buf = [0u8; 8];
        cursor
            .read_exact(&mut sig_count_buf)
            .expect("ERR_INVALID_PSBT");
        let sig_count = u64::from_le_bytes(sig_count_buf) as usize;
        require!(
            sig_count <= MAX_ITEM_COUNT,
            "ERR_INVALID_PSBT: too many script_sigs"
        );
        let mut final_script_sigs = Vec::with_capacity(sig_count);
        for _ in 0..sig_count {
            let mut flag = [0u8; 1];
            cursor.read_exact(&mut flag).expect("ERR_INVALID_PSBT");
            if flag[0] == 1 {
                let mut script_len_buf = [0u8; 8];
                cursor
                    .read_exact(&mut script_len_buf)
                    .expect("ERR_INVALID_PSBT");
                let script_len = u64::from_le_bytes(script_len_buf) as usize;
                require!(
                    script_len <= MAX_ELEMENT_BYTES,
                    "ERR_INVALID_PSBT: script too large"
                );
                let mut script_bytes = vec![0u8; script_len];
                cursor
                    .read_exact(&mut script_bytes)
                    .expect("ERR_INVALID_PSBT");
                final_script_sigs.push(Some(ScriptBuf::from_bytes(script_bytes)));
            } else {
                final_script_sigs.push(None);
            }
        }

        Self {
            unsigned_tx,
            input_utxos,
            final_script_sigs,
        }
    }

    pub fn extract_tx_bytes_with_sign(&self) -> Vec<u8> {
        let mut signed_tx = self.unsigned_tx.clone();

        for (i, sig) in self.final_script_sigs.iter().enumerate() {
            if let Some(script_sig) = sig {
                signed_tx.input[i].script_sig = script_sig.clone();
            }
        }

        for input in &mut signed_tx.input {
            input.witness = Witness::default();
        }

        serialize(&signed_tx)
    }

    pub fn get_pending_id(&self) -> String {
        self.unsigned_tx.compute_txid().to_string()
    }

    #[allow(unused_variables)]
    pub fn get_hash_to_sign(&self, vin: usize, public_keys: &[bitcoin::PublicKey]) -> [u8; 32] {
        let input_utxo = self.input_utxos[vin]
            .as_ref()
            .expect("ERR_MISSING_INPUT_UTXO");

        let cache = SighashCache::new(self.unsigned_tx.clone());
        cache
            .legacy_signature_hash(
                vin,
                &input_utxo.script_pubkey,
                bitcoin::EcdsaSighashType::All.to_u32(),
            )
            .expect("ERR_SIGHASH")
            .to_raw_hash()
            .to_byte_array()
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

        self.final_script_sigs[sign_index] = Some(script_sig);
    }

    pub fn get_recipient_address(&self) -> Option<String> {
        None
    }
}
