use crate::*;
use std::io;
use std::io::{Cursor, Read, Write};
use std::ops::Add;
use crate::zcash_utils::orchard_policy;
use crate::zcash_utils::orchard_policy::BRIDGE_OVK;
use crate::zcash_utils::transaction::Transaction;
use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, TxOut};
use near_sdk::require;
use zcash_primitives::transaction::components::orchard::read_v5_bundle;
use zcash_primitives::transaction::fees::transparent::{InputSize, OutputView};
use zcash_primitives::transaction::fees::FeeRule;
use zcash_primitives::transaction::{TransactionData, TxVersion};
use zcash_protocol::consensus::{BlockHeight, BranchId};
use zcash_protocol::value::{ZatBalance, Zatoshis};
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
    orchard_bundle: Option<orchard::Bundle<orchard::bundle::Authorized, ZatBalance>>,
    orchard_output: Option<(u64, [u8; 43])>,
}

impl PsbtWrapper {
    pub fn new(
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        orchard_bundle_bytes: Option<Vec<u8>>,
        expiry_height: u32,
        config: &Config,
    ) -> Self {
        require!(!input.is_empty(), "empty input");
        // Allow empty output if we have an orchard bundle (funds go to shielded pool)
        require!(
            !output.is_empty() || orchard_bundle_bytes.is_some(),
            "empty output"
        );

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

        let mut real_outputs = Vec::new();
        // Parse and validate orchard bundle if present
        let orchard_bundle = if let Some(orchard_bundle_bytes) = orchard_bundle_bytes {
            let mut reader = Cursor::new(orchard_bundle_bytes);
            let bundle = read_v5_bundle(&mut reader)
                .expect("Failed to read orchard bundle")
                .expect("Orchard bundle is empty");

            let ovk = orchard::keys::OutgoingViewingKey::from(BRIDGE_OVK);

            for action_idx in 0..bundle.actions().len() {
                if let Some((note, addr, _memo)) = bundle.recover_output_with_ovk(action_idx, &ovk)
                {
                    let value = note.value().inner();
                    if value > 0 {
                        real_outputs.push((value, addr.to_raw_address_bytes()));
                    }
                }
            }

            require!(
                real_outputs.len() == 1,
                format!(
                    "Expected exactly 1 non-zero Orchard output, found {}",
                    real_outputs.len()
                )
            );

            // If no expected values provided, enforce minimum actions per Orchard protocol
            require!(
                bundle.actions().len() >= orchard_policy::MIN_ACTIONS,
                format!(
                    "Orchard bundle must have at least {} actions, got {}",
                    orchard_policy::MIN_ACTIONS,
                    bundle.actions().len()
                )
            );

            Some(bundle)
        } else {
            None
        };

        Self {
            branch_id: get_branch_id(expiry_height, config),
            expiry_height,
            vout,
            vin,
            inputs_utxo: inputs,
            orchard_bundle,
            orchard_output: real_outputs.pop(),
        }
    }

    pub fn validate_orchard_bundle(
        &self,
        expected_addr: String,
        expected_amt: u64,
        min_change_amount: u64,
        chain: network::Chain,
    ) {
        orchard_policy::validate_orchard_bundle(
            &self.orchard_bundle.clone().unwrap(),
            &expected_addr,
            expected_amt,
            min_change_amount,
            &chain,
            self.orchard_output.clone().unwrap(),
        );
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
            orchard_bundle: original_psbt.orchard_bundle,
            orchard_output: original_psbt.orchard_output,
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

    pub fn has_orchard_bundle(&self) -> bool {
        self.orchard_bundle.is_some()
    }

    /// Get the Orchard output amount by recovering it with the bridge OVK.
    /// Returns the amount in zatoshis (satoshis for ZCash).
    /// Panics if there is no Orchard bundle.
    pub fn get_orchard_output_amount(&self) -> u128 {
        let (amount, _addr) = self
            .orchard_output
            .as_ref()
            .expect("No Orchard bundle present");
        *amount as u128
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

    pub fn add_extra_outputs(&self, actual_received_amounts: &mut Vec<u128>) -> u128 {
        if let Some((amount, _addr)) = self.orchard_output.clone() {
            actual_received_amounts.push(amount as u128);
            return amount as u128;
        }
        
        return 0;
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
        let version: u8 = 3;
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

        // Serialize orchard bundle if present
        if let Some(ref bundle) = self.orchard_bundle {
            zcash_primitives::transaction::components::orchard::write_v5_bundle(
                Some(bundle),
                &mut buf,
            )
            .unwrap();
        } else {
            // Write empty bundle marker
            zcash_primitives::transaction::components::orchard::write_v5_bundle(None, &mut buf)
                .unwrap();
        }

        if let Some(orchard_output) = self.orchard_output {
            buf.write_all(&[1u8; 1]).unwrap();
            buf.write_all(&orchard_output.0.to_le_bytes()).unwrap();
            buf.write_all(&orchard_output.1).unwrap();
        } else {
            buf.write_all(&[0u8; 1]).unwrap();
        }

        buf
    }
    pub fn serialize(&self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let bytes = hex::decode(psbt_hex).unwrap();
        let mut rdr = Cursor::new(bytes);
        let version = read_u8(&mut rdr).unwrap();
        let branch_id = if version >= 2 {
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

        let orchard_bundle = if version >= 3 {
            read_v5_bundle(&mut rdr).unwrap()
        } else {
            None
        };

        let is_some = read_u8(&mut rdr).unwrap();
        let orchard_output = if is_some == 1 {
            let amount = read_u64_le(&mut rdr).unwrap();
            let mut addr = [0u8; 43];
            for i in 0..43 {
                addr[i] = read_u8(&mut rdr).unwrap();
            }

            Some((amount, addr))
        } else {
            None
        };

        Self {
            branch_id,
            expiry_height,
            vin,
            vout,
            inputs_utxo: inputs,
            orchard_bundle,
            orchard_output,
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

        // Here we encode the Zcash transaction with orchard bundle so it can be submited to the network
        let inner_tx = TransactionData::from_parts(
            TxVersion::V5,
            self.branch_id,
            0,
            BlockHeight::from(self.expiry_height),
            Some(transparent_bundle),
            None,
            None,
            self.orchard_bundle.clone(),
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
            self.orchard_bundle.clone(),
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

        *zcash_primitives::transaction::sighash::signature_hash(&tx_data, &sig_input, &txid_parts)
            .as_ref()
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
        let orchard_action_count = self
            .orchard_bundle
            .as_ref()
            .map(|bundle| bundle.actions().len())
            .unwrap_or(0);

        fee_rule
            .fee_required(
                &zcash_protocol::consensus::MainNetwork,
                BlockHeight::from_u32(0u32),
                vec![InputSize::STANDARD_P2PKH; self.vin.len()],
                self.vout.iter().map(|i| i.serialized_size()),
                0, // sapling_input_count
                0, // sapling_output_count
                orchard_action_count,
            )
            .unwrap()
    }
}

fn get_branch_id(expiry_height: u32, config: &Config) -> BranchId {
    BranchId::Nu6_1
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
