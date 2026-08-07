use crate::network;
use bitcoin::hashes::Hash;
use bitcoin::{absolute, ScriptBuf, TxOut, Txid};
use zcash_primitives::transaction::{Transaction as ZCashTransaction, TransactionData, TxVersion};
use zcash_protocol::consensus::{BlockHeight, BranchId};
use zcash_transparent::builder::{SpendInfo, TransparentBuilder, TransparentInputInfo};
use zcash_transparent::bundle::Authorized;

pub struct TransparentUnauthorized;
impl zcash_primitives::transaction::Authorization for TransparentUnauthorized {
    type TransparentAuth = zcash_transparent::builder::Unauthorized;
    type SaplingAuth = sapling_crypto::bundle::Authorized;
    type OrchardAuth = orchard::bundle::Authorized;
}

#[derive(Debug, PartialEq)]
pub struct Transaction {
    pub inner_tx: ZCashTransaction,
}

impl Transaction {
    pub fn compute_txid(&self) -> Txid {
        Txid::from_byte_array(*self.inner_tx.txid().as_ref())
    }

    pub fn output(&self) -> Vec<TxOut> {
        let outputs = self.inner_tx.transparent_bundle().unwrap().vout.clone();
        outputs
            .into_iter()
            .map(|o| bitcoin::TxOut {
                value: bitcoin::Amount::from_sat(o.value().into_u64()),
                script_pubkey: ScriptBuf::from(bitcoin::Script::from_bytes(
                    &o.script_pubkey().0 .0,
                )),
            })
            .collect()
    }

    pub fn lock_time(&self) -> absolute::LockTime {
        let lock_time = self.inner_tx.lock_time();
        absolute::LockTime::from_consensus(lock_time)
    }

    pub fn encode(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut buf = Vec::new();
        self.inner_tx.write(&mut buf)?;

        Ok(buf)
    }

    pub fn decode(data: &[u8], _chain: &network::Chain) -> Result<Self, std::io::Error> {
        let mut cursor = std::io::Cursor::new(data);
        // `Transaction::read` reads the branch_id from the tx header for v5 and
        // v6 transactions (which is all the bridge ever handles). The explicit
        // `consensus_branch_id` argument only affects legacy v3/v4 parsing, so
        // any post-NU5 variant is a safe placeholder here.
        let tx = ZCashTransaction::read(&mut cursor, BranchId::Nu6_3)?;
        Ok(Self { inner_tx: tx })
    }

    pub fn get_transparent_builder(
        vin: &[zcash_transparent::bundle::TxIn<Authorized>],
        vout: &[zcash_transparent::bundle::TxOut],
        input: &[zcash_transparent::bundle::TxOut],
        public_keys: &[bitcoin::PublicKey],
    ) -> TransparentBuilder {
        let mut builder = zcash_transparent::builder::TransparentBuilder::empty();

        for index in 0..vin.len() {
            let input_info = TransparentInputInfo::from_parts(
                vin[index].prevout().clone(),
                input[index].clone(),
                SpendInfo::P2pkh {
                    pubkey: public_keys[index].inner,
                },
            )
            .unwrap();
            builder.add_input(input_info);
        }

        for output in vout {
            let script_bytes = &output.script_pubkey().0 .0;
            let script = bitcoin::Script::from_bytes(script_bytes);
            let to = if script.is_p2sh() {
                zcash_transparent::address::TransparentAddress::ScriptHash(
                    script_bytes[2..22].try_into().unwrap(),
                )
            } else if script.is_p2pkh() {
                zcash_transparent::address::TransparentAddress::PublicKeyHash(
                    script_bytes[3..23].try_into().unwrap(),
                )
            } else {
                near_sdk::env::panic_str(
                    "ERR_UNSUPPORTED_OUTPUT_SCRIPT: only P2PKH and P2SH transparent outputs are supported",
                )
            };
            builder.add_output(&to, output.value()).unwrap();
        }

        builder
    }

    pub fn to_zcash_tx(
        vin: &[zcash_transparent::bundle::TxIn<Authorized>],
        vout: &[zcash_transparent::bundle::TxOut],
        input: &[zcash_transparent::bundle::TxOut],
        expiry_height: u32,
        public_keys: &[bitcoin::PublicKey],
        branch_id: BranchId,
    ) -> TransactionData<TransparentUnauthorized> {
        let transparent_bundle = Self::get_transparent_builder(vin, vout, input, public_keys)
            .build()
            .unwrap();

        let lock_time = 0;
        let expiry_height = BlockHeight::from_u32(expiry_height);

        // Post-NU6.3 v5 stays consensus-valid (ZIP 229), but shielded value must
        // enter the Ironwood pool (ZIP 2006) and only the v6 format carries an
        // Ironwood bundle, so at Nu6_3 every bridge tx is built as v6 to keep the
        // sighash and broadcast paths on one format.
        if matches!(branch_id, BranchId::Nu6_3) {
            TransactionData::from_parts_v6(
                branch_id,
                lock_time,
                expiry_height,
                Some(transparent_bundle),
                None,
                None,
                None,
            )
        } else {
            TransactionData::from_parts(
                TxVersion::V5,
                branch_id,
                lock_time,
                expiry_height,
                Some(transparent_bundle),
                None,
                None,
                None,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zcash_protocol::value::Zatoshis;
    use zcash_script::script::Code;
    use zcash_transparent::address::Script;
    use zcash_transparent::bundle::TxOut as ZcashTxOut;

    fn p2sh_script() -> Vec<u8> {
        let mut s = vec![0xa9, 0x14];
        s.extend_from_slice(&[0x11u8; 20]);
        s.push(0x87);
        s
    }

    fn p2pkh_script() -> Vec<u8> {
        let mut s = vec![0x76, 0xa9, 0x14];
        s.extend_from_slice(&[0x22u8; 20]);
        s.push(0x88);
        s.push(0xac);
        s
    }

    #[test]
    fn transparent_builder_preserves_output_script_pubkey() {
        for script in [p2sh_script(), p2pkh_script()] {
            let vout = vec![ZcashTxOut::new(
                Zatoshis::from_u64(50_000).unwrap(),
                Script(Code(script.clone())),
            )];
            let bundle = Transaction::get_transparent_builder(&[], &vout, &[], &[])
                .build()
                .unwrap();
            assert_eq!(bundle.vout[0].script_pubkey().0 .0, script);
        }
    }

    #[test]
    #[should_panic(expected = "ERR_UNSUPPORTED_OUTPUT_SCRIPT")]
    fn transparent_builder_rejects_non_transparent_output_script() {
        let mut witness_script = vec![0x00, 0x14];
        witness_script.extend_from_slice(&[0x33u8; 20]);
        let vout = vec![ZcashTxOut::new(
            Zatoshis::from_u64(50_000).unwrap(),
            Script(Code(witness_script)),
        )];
        let _ = Transaction::get_transparent_builder(&[], &vout, &[], &[]).build();
    }
}
