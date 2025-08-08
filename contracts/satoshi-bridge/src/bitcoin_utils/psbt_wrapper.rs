use crate::*;

use bitcoin::absolute::LockTime;
use bitcoin::psbt::Psbt;
use bitcoin::transaction::Version;
use bitcoin::{OutPoint, Transaction, TxIn, TxOut};
use near_sdk::require;

pub struct PsbtWrapper {
    pub psbt: Psbt,
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

    pub fn set_input_utxo(&mut self, input_utxo: Vec<TxOut>) {
        input_utxo
            .iter()
            .enumerate()
            .for_each(|(i, v)| self.psbt.inputs[i].witness_utxo = Some(v.clone()));
    }

    pub fn get_input(&self) -> &Vec<TxIn> {
        &self.psbt.unsigned_tx.input
    }

    pub fn get_output(&self) -> &Vec<TxOut> {
        &self.psbt.unsigned_tx.output
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

    pub fn extract_tx(&self) -> Transaction {
        self.psbt.clone().extract_tx().expect("extract_tx failed")
    }

    pub fn get_pending_id(&self) -> String {
        self.psbt
            .clone()
            .extract_tx()
            .unwrap()
            .compute_txid()
            .to_string()
    }
}
