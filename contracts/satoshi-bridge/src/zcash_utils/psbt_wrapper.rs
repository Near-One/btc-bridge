use crate::*;

use bitcoin::absolute::LockTime;
use bitcoin::psbt::Psbt;
use bitcoin::sighash::SighashCache;
use bitcoin::transaction::Version;
use bitcoin::{secp256k1, OutPoint, Transaction, TxIn, TxOut, Witness};
use near_sdk::require;
use zcash_primitives::transaction::fees::transparent::{InputView, OutputView};
use zcash_primitives::transaction::fees::FeeRule;
use zcash_protocol::consensus::BlockHeight;
use zcash_protocol::value::Zatoshis;

pub struct PsbtWrapper {
    psbt: Psbt,
    expiry_height: u32,
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

        Self {
            psbt,
            expiry_height,
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
        }
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
        let h_encode = hex::encode(self.expiry_height.to_le_bytes());
        format!("{}{}", h_encode, self.psbt.serialize_hex())
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let h_hex = hex::decode(&psbt_hex[..8]).unwrap();
        let psbt_bytes = hex::decode(&psbt_hex[8..]).unwrap();
        Self {
            psbt: Psbt::deserialize(&psbt_bytes).expect("ERR_INVALID_PSBT_HEX"),
            expiry_height: u32::from_le_bytes(h_hex.try_into().unwrap()),
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

    pub fn get_min_fee(&self, public_key: &bitcoin::PublicKey) -> Zatoshis {
        let fee_rule = zcash_primitives::transaction::fees::zip317::FeeRule::standard();
        let transparent_builder =
            WrappedTransaction::get_transparent_builder(&self.psbt, public_key);
        fee_rule
            .fee_required(
                &zcash_protocol::consensus::MainNetwork,
                BlockHeight::from_u32(0u32),
                transparent_builder
                    .inputs()
                    .iter()
                    .map(|i| i.serialized_size()),
                transparent_builder
                    .outputs()
                    .iter()
                    .map(|i| i.serialized_size()),
                0,
                0,
                0,
            )
            .unwrap()
    }
}
