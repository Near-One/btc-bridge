use crate::network::ORCHARD_RAW_ADDRESS_SIZE;
use crate::zcash_utils::orchard_policy::{self, OrchardOutput, ParsedOrchardBundle};
use crate::zcash_utils::transaction::{Transaction, TransparentUnauthorized};
use crate::*;
use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, TxOut};
use near_sdk::{env, require};
use orchard::bundle::BundleVersion;
use std::io;
use std::io::{Cursor, Read, Write};
use zcash_primitives::transaction::fees::transparent::{InputSize, OutputView};
use zcash_primitives::transaction::fees::FeeRule;
use zcash_primitives::transaction::{TransactionData, TransactionDigest, TxVersion};
use zcash_protocol::consensus::{BlockHeight, BranchId};
use zcash_protocol::value::Zatoshis;
use zcash_script::script::Code;
use zcash_transparent::address::Script;
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
    orchard: Option<ParsedOrchardBundle>,
    recipient_address: Option<String>,
}

impl PsbtWrapper {
    pub fn new(
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        orchard_bundle_bytes: Option<Vec<u8>>,
        expiry_height: u32,
        current_height: u32,
        recipient_address: Option<String>,
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
            .map(|o| {
                ZcashTxOut::new(
                    Zatoshis::from_u64(o.value.to_sat()).unwrap(),
                    Script(Code(o.script_pubkey.to_bytes())),
                )
            })
            .collect();

        let vin: Vec<ZcashTxIn<Authorized>> = input
            .into_iter()
            .map(|i| {
                ZcashTxIn::from_parts(
                    zcash_transparent::bundle::OutPoint::new(*i.txid.as_byte_array(), i.vout),
                    Script::default(),
                    sequence.0,
                )
            })
            .collect();

        let inputs =
            vec![ZcashTxOut::new(Zatoshis::from_u64(0).unwrap(), Script::default()); vin.len()];

        let branch_id = get_branch_id(current_height, config);
        let orchard = orchard_policy::extract_orchard_bundle(
            orchard_bundle_bytes,
            bundle_version_for(branch_id),
        )
        .unwrap_or_else(|_| {
            env::panic_str("ERR_INVALID_ORCHARD_BUNDLE: failed to extract Orchard bundle")
        });

        Self {
            branch_id,
            expiry_height,
            vout,
            vin,
            inputs_utxo: inputs,
            orchard,
            recipient_address,
        }
    }

    pub fn validate_orchard_bundle(&self, expected_addr: String, chain: network::Chain) {
        orchard_policy::validate_orchard_bundle(
            self.orchard.as_ref().unwrap_or_else(|| {
                env::panic_str("ERR_NO_ORCHARD_BUNDLE: Orchard bundle is required for validation")
            }),
            &expected_addr,
            &chain,
        )
        .unwrap_or_else(|_| {
            env::panic_str("ERR_ORCHARD_VALIDATION: Orchard bundle validation failed")
        });
    }

    pub fn from_original_psbt(
        original_psbt: PsbtWrapper,
        output: Vec<TxOut>,
        orchard_bundle_bytes: Option<Vec<u8>>,
        expiry_height: u32,
        current_height: u32,
        config: &Config,
    ) -> Self {
        let vout = if output.is_empty() {
            original_psbt.vout.clone()
        } else {
            output
                .clone()
                .into_iter()
                .map(|o| {
                    ZcashTxOut::new(
                        Zatoshis::from_u64(o.value.to_sat()).unwrap(),
                        Script(Code(o.script_pubkey.to_bytes())),
                    )
                })
                .collect()
        };

        let branch_id = get_branch_id(current_height, config);
        let orchard = orchard_policy::extract_orchard_bundle(
            orchard_bundle_bytes,
            bundle_version_for(branch_id),
        )
        .unwrap_or_else(|_| {
            env::panic_str("ERR_INVALID_ORCHARD_BUNDLE: failed to extract Orchard bundle")
        });

        Self {
            branch_id,
            expiry_height,
            vin: original_psbt.vin,
            vout,
            inputs_utxo: original_psbt.inputs_utxo,
            orchard,
            recipient_address: original_psbt.recipient_address,
        }
    }

    pub fn set_input_utxo(&mut self, input_utxo: Vec<TxOut>) {
        input_utxo.iter().enumerate().for_each(|(i, v)| {
            self.inputs_utxo[i] = ZcashTxOut::new(
                Zatoshis::from_u64(v.value.to_sat()).unwrap(),
                Script(Code(v.script_pubkey.to_bytes())),
            )
        });
    }

    pub fn get_input_num(&self) -> usize {
        self.vin.len()
    }

    pub fn get_output_num(&self) -> usize {
        self.vout.len()
    }

    pub fn has_orchard_bundle(&self) -> bool {
        self.orchard.is_some()
    }

    /// Get the Orchard output amount by recovering it with the bridge OVK.
    /// Returns the amount in zatoshis (satoshis for ZCash).
    /// Panics if there is no Orchard bundle.
    pub fn get_orchard_output_amount(&self) -> u128 {
        self.orchard
            .as_ref()
            .unwrap_or_else(|| env::panic_str("No Orchard bundle present"))
            .amount()
    }

    pub fn get_utxo_storage_keys(&self) -> Vec<String> {
        self.vin
            .clone()
            .into_iter()
            .map(|out_point| {
                generate_utxo_storage_key(
                    out_point.prevout().txid().to_string(),
                    out_point.prevout().n(),
                )
            })
            .collect()
    }

    pub fn add_extra_outputs(&self, actual_received_amounts: &mut Vec<u128>) -> u128 {
        if let Some(orchard) = &self.orchard {
            actual_received_amounts.push(orchard.amount());
            return orchard.amount();
        }

        0
    }

    pub fn get_output(&self) -> Vec<TxOut> {
        self.vout
            .clone()
            .into_iter()
            .map(|i| TxOut {
                value: bitcoin::Amount::from_sat(i.value().into_u64()),
                script_pubkey: ScriptBuf::from_bytes(i.script_pubkey().0 .0.clone()),
            })
            .collect()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::new();
        let version: u8 = 4;
        buf.push(version);
        match self.branch_id {
            BranchId::Nu6 => buf.write_all(&[7u8; 1]).unwrap(),
            BranchId::Nu6_1 => buf.write_all(&[8u8; 1]).unwrap(),
            BranchId::Nu6_2 => buf.write_all(&[9u8; 1]).unwrap(),
            BranchId::Nu6_3 => buf.write_all(&[10u8; 1]).unwrap(),
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

        if let Some(orchard) = &self.orchard {
            if is_ironwood(self.branch_id) {
                zcash_primitives::transaction::components::orchard::write_v6_bundle(
                    Some(&orchard.bundle),
                    &mut buf,
                )
                .unwrap();
            } else {
                zcash_primitives::transaction::components::orchard::write_v5_bundle(
                    Some(&orchard.bundle),
                    &mut buf,
                )
                .unwrap();
            }

            buf.write_all(&[1u8; 1]).unwrap();
            buf.write_all(&orchard.output.amount.to_le_bytes()).unwrap();
            buf.write_all(&orchard.output.recipient_addr).unwrap();
        } else {
            buf.write_all(&[0u8; 1]).unwrap();
        }

        if let Some(recipient_address) = &self.recipient_address {
            buf.write_all(&[1u8; 1]).unwrap();
            let recipient_address_bytes = recipient_address.as_bytes();

            let len = recipient_address_bytes.len() as u64;
            buf.write_all(&len.to_le_bytes()).unwrap();

            buf.write_all(recipient_address_bytes).unwrap();
        } else {
            buf.write_all(&[0u8; 1]).unwrap();
        }

        buf
    }
    pub fn serialize(&self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn deserialize(psbt_hex: &String) -> Self {
        let bytes = hex::decode(psbt_hex)
            .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT_HEX: failed to decode hex"));
        let mut rdr = Cursor::new(bytes);
        let version = read_u8(&mut rdr)
            .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read version"));
        let branch_id = if version >= 2 {
            let branch_id_u8 = read_u8(&mut rdr)
                .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read branch_id"));
            match branch_id_u8 {
                7 => BranchId::Nu6,
                8 => BranchId::Nu6_1,
                9 => BranchId::Nu6_2,
                10 => BranchId::Nu6_3,
                _ => env::panic_str("ERR_INVALID_PSBT: unsupported branch_id"),
            }
        } else {
            BranchId::Nu6_1
        };

        let expiry_height = read_u32_le(&mut rdr)
            .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read expiry_height"));

        let vin_len = read_u64_le(&mut rdr)
            .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read vin length"))
            as usize;
        let mut vin = Vec::with_capacity(vin_len);
        for _ in 0..vin_len {
            vin.push(
                ZcashTxIn::<Authorized>::read(&mut rdr)
                    .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read vin")),
            );
        }

        let vout_len = read_u64_le(&mut rdr)
            .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read vout length"))
            as usize;
        let mut vout = Vec::with_capacity(vout_len);
        for _ in 0..vout_len {
            vout.push(
                ZcashTxOut::read(&mut rdr)
                    .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read vout")),
            );
        }

        let inputs_len = read_u64_le(&mut rdr)
            .unwrap_or_else(|_| env::panic_str("ERR_INVALID_PSBT: failed to read inputs length"))
            as usize;
        let mut inputs = Vec::with_capacity(inputs_len);
        for _ in 0..inputs_len {
            inputs.push(
                ZcashTxOut::read(&mut rdr).unwrap_or_else(|_| {
                    env::panic_str("ERR_INVALID_PSBT: failed to read input utxo")
                }),
            );
        }

        let orchard_bundle = if version >= 3 {
            let bundle_version = bundle_version_for(branch_id);
            let result = if is_ironwood(branch_id) {
                zcash_primitives::transaction::components::orchard::read_v6_bundle(
                    &mut rdr,
                    bundle_version,
                )
            } else {
                zcash_primitives::transaction::components::orchard::read_v5_bundle(
                    &mut rdr,
                    bundle_version,
                )
            };
            result.unwrap_or_else(|_| {
                env::panic_str("ERR_INVALID_PSBT: failed to read Orchard bundle")
            })
        } else {
            None
        };

        let orchard = if let Some(orchard_bundle) = orchard_bundle {
            let is_some = read_u8(&mut rdr).unwrap_or_else(|_| {
                env::panic_str("ERR_INVALID_PSBT: failed to read orchard_output flag")
            });
            if is_some == 1 {
                let amount = read_u64_le(&mut rdr).unwrap_or_else(|_| {
                    env::panic_str("ERR_INVALID_PSBT: failed to read orchard amount")
                });
                let mut addr = [0u8; ORCHARD_RAW_ADDRESS_SIZE];
                for addr_byte in &mut addr {
                    *addr_byte = read_u8(&mut rdr).unwrap_or_else(|_| {
                        env::panic_str("ERR_INVALID_PSBT: failed to read orchard address")
                    });
                }

                Some(ParsedOrchardBundle {
                    bundle: orchard_bundle,
                    output: OrchardOutput {
                        amount,
                        recipient_addr: addr,
                    },
                })
            } else {
                None
            }
        } else {
            None
        };

        let recipient_address = if version >= 3 {
            let is_some = read_u8(&mut rdr).unwrap_or_else(|_| {
                env::panic_str("ERR_INVALID_PSBT: failed to read recipient_address flag")
            });
            if is_some == 1 {
                Some(read_string(&mut rdr).unwrap_or_else(|_| {
                    env::panic_str("ERR_INVALID_PSBT: failed to read recipient_address")
                }))
            } else {
                None
            }
        } else {
            None
        };

        Self {
            branch_id,
            expiry_height,
            vin,
            vout,
            inputs_utxo: inputs,
            orchard,
            recipient_address,
        }
    }

    pub fn extract_tx_bytes_with_sign(self) -> Vec<u8> {
        self.get_zcash_tx()
            .encode()
            .unwrap_or_else(|_| env::panic_str("ERR_TX_ENCODE: failed to encode Zcash transaction"))
    }

    pub fn get_zcash_tx(self) -> Transaction {
        let transparent_bundle = zcash_transparent::bundle::Bundle {
            vin: self.vin.clone(),
            vout: self.vout.clone(),
            authorization: zcash_transparent::bundle::Authorized,
        };

        let branch_id = self.branch_id;
        let expiry = BlockHeight::from(self.expiry_height);
        let shielded = self.orchard.map(|b| b.bundle);

        // NU6.3+ transactions must be v6, with the shielded bundle routed to the
        // Ironwood slot (adding new value into the Orchard pool is a consensus
        // violation post-NU6.3). Pre-NU6.3 keeps the v5 + Orchard-slot path.
        let inner_tx = if is_ironwood(branch_id) {
            TransactionData::from_parts_v6(
                branch_id,
                0,
                expiry,
                Some(transparent_bundle),
                None,     // sapling
                None,     // orchard slot (empty; new value cannot enter Orchard)
                shielded, // ironwood slot
            )
        } else {
            TransactionData::from_parts(
                TxVersion::V5,
                branch_id,
                0,
                expiry,
                Some(transparent_bundle),
                None, // sprout
                None, // sapling
                shielded,
            )
        }
        .freeze()
        .unwrap_or_else(|_| {
            env::panic_str("ERR_TX_FREEZE: failed to freeze Zcash transaction data")
        });

        Transaction { inner_tx }
    }

    pub fn get_pending_id(self) -> String {
        self.get_zcash_tx().compute_txid().to_string()
    }

    fn tx_digest<D: TransactionDigest<TransparentUnauthorized>>(
        &self,
        tx_data: &TransactionData<TransparentUnauthorized>,
        digester: D,
    ) -> D::Digest {
        let version = tx_data.version();
        // Post-NU6.3 the shielded bundle lives in the Ironwood slot, so route the
        // recovered bundle to `digest_ironwood`; earlier epochs still commit it
        // via `digest_orchard`. Both slots are otherwise `None`.
        let shielded = self.orchard.as_ref().map(|b| &b.bundle);
        let (orchard_bundle, ironwood_bundle) = if is_ironwood(self.branch_id) {
            (None, shielded)
        } else {
            (shielded, None)
        };
        digester.combine(
            digester.digest_header(
                version,
                tx_data.consensus_branch_id(),
                tx_data.lock_time(),
                tx_data.expiry_height(),
            ),
            digester.digest_transparent(tx_data.transparent_bundle()),
            digester.digest_sapling(version, None),
            digester.digest_orchard(version, orchard_bundle),
            digester.digest_ironwood(ironwood_bundle),
        )
    }

    #[allow(unused_variables)]
    pub fn get_hash_to_sign(&self, vin: usize, public_keys: &[bitcoin::PublicKey]) -> [u8; 32] {
        let tx_data = WrappedTransaction::to_zcash_tx(
            &self.vin,
            &self.vout,
            &self.inputs_utxo,
            self.expiry_height,
            public_keys,
            self.branch_id,
        );
        let txid_parts =
            self.tx_digest(&tx_data, zcash_primitives::transaction::txid::TxIdDigester);

        let script = self.inputs_utxo[vin].script_pubkey();
        let transparent_bundle = tx_data.transparent_bundle().unwrap_or_else(|| {
            env::panic_str("ERR_NO_TRANSPARENT_BUNDLE: missing transparent bundle")
        });
        let sig_input = zcash_primitives::transaction::sighash::SignableInput::Transparent(
            zcash_transparent::sighash::SignableInput::from_parts(
                transparent_bundle,
                SighashType::ALL,
                vin,
                script,
                script,
                self.inputs_utxo[vin].value(),
            )
            .unwrap_or_else(|_| env::panic_str("ERR_SIGNABLE_INPUT: invalid input index")),
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

        let prevout = self.vin[sign_index].prevout().clone();
        let sequence = self.vin[sign_index].sequence();
        self.vin[sign_index] =
            ZcashTxIn::from_parts(prevout, Script(Code(script_sig.to_bytes())), sequence);
    }

    pub fn get_min_fee(&self) -> Zatoshis {
        let action_count = self
            .orchard
            .as_ref()
            .map(|orchard| orchard.bundle.actions().len())
            .unwrap_or(0);

        let (orchard_action_count, ironwood_action_count) = if is_ironwood(self.branch_id) {
            (0, action_count)
        } else {
            (action_count, 0)
        };

        zip317_min_fee(
            self.vin.len(),
            self.vout.iter().map(|i| i.serialized_size()).collect(),
            orchard_action_count,
            ironwood_action_count,
        )
    }

    pub fn get_recipient_address(&self) -> Option<String> {
        self.recipient_address.clone()
    }
}

/// ZIP-317 minimum fee for a transaction with the given structure (`input_count`
/// standard P2PKH inputs, the given transparent output sizes, and Orchard/Ironwood
/// action counts). Height/branch_id do not affect the fee; the caller decides which
/// pool the shielded action count belongs to.
pub(crate) fn zip317_min_fee(
    input_count: usize,
    transparent_output_sizes: Vec<usize>,
    orchard_action_count: usize,
    ironwood_action_count: usize,
) -> Zatoshis {
    zcash_primitives::transaction::fees::zip317::FeeRule::standard()
        .fee_required(
            &zcash_protocol::consensus::MainNetwork,
            BlockHeight::from_u32(0u32),
            vec![InputSize::STANDARD_P2PKH; input_count],
            transparent_output_sizes,
            0, // sapling_input_count
            0, // sapling_output_count
            orchard_action_count,
            ironwood_action_count,
        )
        .unwrap()
}

fn get_branch_id(current_height: u32, config: &Config) -> BranchId {
    config.chain.get_branch_id(current_height)
}

/// Whether transactions at this branch id must be built as v6 with the shielded
/// bundle routed into the Ironwood slot instead of the Orchard slot.
pub(crate) fn is_ironwood(branch_id: BranchId) -> bool {
    matches!(branch_id, BranchId::Nu6_3)
}

/// Selects the `BundleVersion` used when (de)serializing a standalone Orchard-or-
/// Ironwood bundle for the given consensus branch. The bundle wire format is
/// identical across variants; the `BundleVersion` fixes flag-byte semantics
/// (cross-address bit) and circuit / proof-size expectations.
pub(crate) fn bundle_version_for(branch_id: BranchId) -> BundleVersion {
    match branch_id {
        BranchId::Nu6_2 => BundleVersion::orchard_v2(),
        BranchId::Nu6_3 => BundleVersion::ironwood_v3(),
        _ => BundleVersion::orchard_insecure_v1(),
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

fn read_string<R: Read>(r: &mut R) -> io::Result<String> {
    let len = read_u64_le(r)? as usize;
    let mut recipient_address_bytes = vec![];

    for _ in 0..len {
        recipient_address_bytes.push(read_u8(r)?);
    }

    String::from_utf8(recipient_address_bytes.to_vec())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn read_u64_le<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}
