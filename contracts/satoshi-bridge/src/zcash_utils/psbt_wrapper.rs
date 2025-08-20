use crate::*;
use std::io;
use std::io::{Cursor, Read, Write};

use crate::zcash_utils::transaction::Transaction;
use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, TxOut};
use near_sdk::require;
use zcash_primitives::transaction::fees::transparent::{InputSize, OutputView};
use zcash_primitives::transaction::fees::FeeRule;
use zcash_primitives::transaction::{TransactionData, TxVersion};
use zcash_protocol::consensus::{BlockHeight, BranchId};
use zcash_protocol::value::Zatoshis;
use zcash_transparent::bundle::Authorized;
use zcash_transparent::bundle::TxIn as ZcashTxIn;
use zcash_transparent::bundle::TxOut as ZcashTxOut;
use zcash_transparent::sighash::SighashType;

pub struct PsbtWrapper {
    branch_id: BranchId,
    expiry_height: u32,
    vin: Vec<ZcashTxIn<Authorized>>,
    vout: Vec<ZcashTxOut>,
    inputs_utxo: Vec<ZcashTxOut>,
}

impl PsbtWrapper {
    pub fn new(
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        expiry_height: u32,
        config: &Config,
    ) -> Self {
        require!(!input.is_empty(), "empty input");
        require!(!output.is_empty(), "empty output");

        let sequence = bitcoin::Sequence::MAX;
        let vout = output
            .clone()
            .into_iter()
            .map(|o| ZcashTxOut {
                value: Zatoshis::from_u64(o.value.to_sat()).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script(o.script_pubkey.to_bytes()),
            })
            .collect();

        let vin: Vec<ZcashTxIn<Authorized>> = input
            .into_iter()
            .map(|i| ZcashTxIn {
                prevout: zcash_transparent::bundle::OutPoint::new(*i.txid.as_byte_array(), i.vout),
                script_sig: zcash_primitives::legacy::Script::default(),
                sequence: sequence.0,
            })
            .collect();

        let inputs = vec![
            ZcashTxOut {
                value: Zatoshis::from_u64(0).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script::default(),
            };
            vin.len()
        ];

        Self {
            branch_id: get_branch_id(expiry_height, config),
            expiry_height,
            vout,
            vin,
            inputs_utxo: inputs,
        }
    }

    pub fn from_original_psbt(
        original_psbt: PsbtWrapper,
        output: Vec<TxOut>,
        expiry_height: u32,
        config: &Config,
    ) -> Self {
        let vout = if output.is_empty() {
            original_psbt.vout.clone()
        } else {
            output
                .clone()
                .into_iter()
                .map(|o| ZcashTxOut {
                    value: Zatoshis::from_u64(o.value.to_sat()).unwrap(),
                    script_pubkey: zcash_primitives::legacy::Script(o.script_pubkey.to_bytes()),
                })
                .collect()
        };

        Self {
            branch_id: get_branch_id(expiry_height, config),
            expiry_height,
            vin: original_psbt.vin,
            vout,
            inputs_utxo: original_psbt.inputs_utxo,
        }
    }

    pub fn set_input_utxo(&mut self, input_utxo: Vec<TxOut>) {
        input_utxo.iter().enumerate().for_each(|(i, v)| {
            self.inputs_utxo[i] = ZcashTxOut {
                value: Zatoshis::from_u64(v.value.to_sat()).unwrap(),
                script_pubkey: zcash_primitives::legacy::Script(v.script_pubkey.to_bytes()),
            }
        });
    }

    pub fn get_input_num(&self) -> usize {
        self.vin.len()
    }

    pub fn get_output_num(&self) -> usize {
        self.vout.len()
    }
    pub fn get_utxo_storage_keys(&self) -> Vec<String> {
        self.vin
            .clone()
            .into_iter()
            .map(|out_point| {
                generate_utxo_storage_key(
                    out_point.prevout.txid().to_string(),
                    out_point.prevout.n(),
                )
            })
            .collect()
    }

    pub fn get_output(&self) -> Vec<TxOut> {
        self.vout
            .clone()
            .into_iter()
            .map(|i| TxOut {
                value: bitcoin::Amount::from_sat(i.value.into_u64()),
                script_pubkey: ScriptBuf::from_bytes(i.script_pubkey.0),
            })
            .collect()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::new();
        let version: u8 = 2;
        buf.push(version);
        match self.branch_id {
            BranchId::Nu6 => buf.write_all(&[7u8; 1]).unwrap(),
            BranchId::Nu6_1 => buf.write_all(&[8u8; 1]).unwrap(),
            _ => unreachable!(),
        }
        buf.write_all(&self.expiry_height.to_le_bytes()).unwrap();

        let len = self.vin.len() as u64;
        buf.write_all(&len.to_le_bytes()).unwrap();

        for t in self.vin.clone() {
            t.write(&mut buf).unwrap();
        }

        let len = self.vout.len() as u64;
        buf.write_all(&len.to_le_bytes()).unwrap();

        for t in self.vout.clone() {
            t.write(&mut buf).unwrap();
        }

        let len = self.inputs_utxo.len() as u64;
        buf.write_all(&len.to_le_bytes()).unwrap();

        for t in self.inputs_utxo.clone() {
            t.write(&mut buf).unwrap();
        }

        buf
    }
    pub fn serialize(&self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let bytes = hex::decode(&psbt_hex).unwrap();
        let mut rdr = Cursor::new(bytes);
        let version = read_u8(&mut rdr).unwrap();
        let branch_id = if version == 2 {
            let branch_id_u8 = read_u8(&mut rdr).unwrap();
            match branch_id_u8 {
                7 => BranchId::Nu6,
                8 => BranchId::Nu6_1,
                _ => unreachable!(),
            }
        } else {
            BranchId::Nu6_1
        };

        let expiry_height = read_u32_le(&mut rdr).unwrap();

        let vin_len = read_u64_le(&mut rdr).unwrap() as usize;
        let mut vin = Vec::with_capacity(vin_len);
        for _ in 0..vin_len {
            vin.push(ZcashTxIn::<Authorized>::read(&mut rdr).unwrap());
        }

        let vout_len = read_u64_le(&mut rdr).unwrap() as usize;
        let mut vout = Vec::with_capacity(vout_len);
        for _ in 0..vout_len {
            vout.push(ZcashTxOut::read(&mut rdr).unwrap());
        }

        let inputs_len = read_u64_le(&mut rdr).unwrap() as usize;
        let mut inputs = Vec::with_capacity(inputs_len);
        for _ in 0..inputs_len {
            inputs.push(ZcashTxOut::read(&mut rdr).unwrap());
        }

        Self {
            branch_id,
            expiry_height,
            vin,
            vout,
            inputs_utxo: inputs,
        }
    }

    pub fn extract_tx_bytes_with_sign(&self) -> Vec<u8> {
        self.get_zcash_tx().encode().unwrap()
    }

    pub fn get_zcash_tx(&self) -> Transaction {
        let transparent_bundle = zcash_transparent::bundle::Bundle {
            vin: self.vin.clone(),
            vout: self.vout.clone(),
            authorization: zcash_transparent::bundle::Authorized,
        };

        let inner_tx = TransactionData::from_parts(
            TxVersion::V5,
            self.branch_id,
            0,
            BlockHeight::from(self.expiry_height),
            Some(transparent_bundle),
            None,
            None,
            None,
        )
        .freeze()
        .unwrap();

        Transaction { inner_tx }
    }

    pub fn get_pending_id(&self) -> String {
        self.get_zcash_tx().compute_txid().to_string()
    }

    #[allow(unused_variables)]
    pub fn get_hash_to_sign(&self, vin: usize, public_key: &bitcoin::PublicKey) -> [u8; 32] {
        let tx_data = WrappedTransaction::to_zcash_tx(
            &self.vin,
            &self.vout,
            &self.inputs_utxo,
            self.expiry_height,
            public_key,
            self.branch_id,
        );
        let txid_parts = tx_data.digest(zcash_primitives::transaction::txid::TxIdDigester);
        let script = &self.inputs_utxo[vin].script_pubkey;
        let sig_input = zcash_primitives::transaction::sighash::SignableInput::Transparent(
            zcash_transparent::sighash::SignableInput::from_parts(
                SighashType::ALL,
                vin,
                script,
                script,
                self.inputs_utxo[vin].value,
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

        self.vin[sign_index].script_sig = zcash_primitives::legacy::Script(script_sig.to_bytes());
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

fn get_branch_id(expiry_height: u32, config: &Config) -> BranchId {
    let current_height = expiry_height - config.expiry_height_gap;
    let nu6_1_block_height = config.nu6_1_block_height;

    match nu6_1_block_height {
        Some(nu6_1_block_height) => {
            if current_height >= nu6_1_block_height {
                BranchId::Nu6_1
            } else {
                BranchId::Nu6
            }
        }
        None => BranchId::Nu6,
    }
}

fn read_u32_le<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u8<R: Read>(r: &mut R) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u64_le<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}
