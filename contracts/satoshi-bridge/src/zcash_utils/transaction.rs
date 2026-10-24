use crate::network;
use bitcoin::hashes::Hash;
use bitcoin::{absolute, ScriptBuf, TxOut, Txid};
use near_sdk::require;
use zcash_primitives::transaction::{Transaction as ZCashTransaction, TransactionData, TxVersion};
use zcash_protocol::consensus::{BlockHeight, BranchId};
use zcash_protocol::value::Zatoshis;
use zcash_transparent::address::Script;
use zcash_transparent::bundle::{Authorized, Bundle, TxIn, TxOut as ZcashTxOut};
use zcash_transparent::sighash::TransparentAuthorizingContext;

#[derive(Debug)]
pub struct TransparentEffects {
    coins: Vec<ZcashTxOut>,
}

impl zcash_transparent::bundle::Authorization for TransparentEffects {
    type ScriptSig = ();
}

impl TransparentAuthorizingContext for TransparentEffects {
    fn input_amounts(&self) -> Vec<Zatoshis> {
        self.coins.iter().map(|coin| coin.value()).collect()
    }

    fn input_scriptpubkeys(&self) -> Vec<Script> {
        self.coins
            .iter()
            .map(|coin| coin.script_pubkey().clone())
            .collect()
    }
}

pub struct TransparentUnauthorized;
impl zcash_primitives::transaction::Authorization for TransparentUnauthorized {
    type TransparentAuth = TransparentEffects;
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

    pub fn build_transparent_bundle(
        vin: &[TxIn<Authorized>],
        vout: &[ZcashTxOut],
        coins: &[ZcashTxOut],
    ) -> Bundle<TransparentEffects> {
        require!(
            vin.len() == coins.len(),
            "ERR_INPUT_COIN_MISMATCH: every input needs the coin it spends"
        );
        require!(
            !vin.is_empty() || !vout.is_empty(),
            "ERR_EMPTY_TRANSPARENT_BUNDLE: no inputs and no outputs"
        );

        for output in vout {
            let script = bitcoin::Script::from_bytes(&output.script_pubkey().0 .0);
            require!(
                script.is_p2pkh() || script.is_p2sh(),
                "ERR_UNSUPPORTED_OUTPUT_SCRIPT: only P2PKH and P2SH transparent outputs are supported"
            );
        }

        Bundle {
            vin: vin
                .iter()
                .map(|txin| TxIn::from_parts(txin.prevout().clone(), (), txin.sequence()))
                .collect(),
            vout: vout.to_vec(),
            authorization: TransparentEffects {
                coins: coins.to_vec(),
            },
        }
    }

    pub fn to_zcash_tx(
        vin: &[TxIn<Authorized>],
        vout: &[ZcashTxOut],
        coins: &[ZcashTxOut],
        expiry_height: u32,
        branch_id: BranchId,
    ) -> TransactionData<TransparentUnauthorized> {
        let transparent_bundle = Self::build_transparent_bundle(vin, vout, coins);

        let lock_time = 0;
        let expiry_height = BlockHeight::from_u32(expiry_height);

        // Post-NU6.3 v5 stays consensus-valid (ZIP 229), but shielded value must
        // enter the Ironwood pool (ZIP 2006) and only the v6 format carries an
        // Ironwood bundle, so from Nu6_3 on every bridge tx is built as v6 to keep
        // the sighash and broadcast paths on one format.
        if crate::zcash_utils::psbt_wrapper::is_ironwood(branch_id) {
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
    use zcash_script::script::Code;

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
            let bundle = Transaction::build_transparent_bundle(&[], &vout, &[]);
            assert_eq!(bundle.vout[0].script_pubkey().0 .0, script);
        }
    }

    /// Pins the transparent sighash against the digest produced while the
    /// bundle was still assembled by `TransparentBuilder` (via
    /// `SpendInfo::P2pkh`). Dropping that path must not move the sighash, or
    /// every Chain Signatures signature would be made over the wrong payload.
    #[test]
    fn transparent_sighash_matches_the_transparent_builder() {
        // The builder only accepted a coin whose scriptPubKey hashed to the
        // spending pubkey, so the pinned digest is tied to this exact script.
        let pubkey = bitcoin::PublicKey::from_slice(
            &hex::decode("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")
                .unwrap(),
        )
        .unwrap();
        let coin = ZcashTxOut::new(
            Zatoshis::from_u64(100_000).unwrap(),
            Script(Code(
                bitcoin::ScriptBuf::new_p2pkh(&pubkey.pubkey_hash()).to_bytes(),
            )),
        );
        let vin = vec![TxIn::<Authorized>::from_parts(
            zcash_transparent::bundle::OutPoint::new([0x44u8; 32], 1),
            Script::default(),
            u32::MAX,
        )];
        let vout = vec![ZcashTxOut::new(
            Zatoshis::from_u64(90_000).unwrap(),
            Script(Code(p2sh_script())),
        )];

        let tx_data = Transaction::to_zcash_tx(
            &vin,
            &vout,
            std::slice::from_ref(&coin),
            2_000_000,
            BranchId::Nu6,
        );
        let txid_parts = tx_data.digest(zcash_primitives::transaction::txid::TxIdDigester);
        let bundle = tx_data.transparent_bundle().unwrap();
        let sig_input = zcash_primitives::transaction::sighash::SignableInput::Transparent(
            zcash_transparent::sighash::SignableInput::from_parts(
                bundle,
                zcash_transparent::sighash::SighashType::ALL,
                0,
                coin.script_pubkey(),
                coin.script_pubkey(),
                coin.value(),
            )
            .unwrap(),
        );

        let sighash = zcash_primitives::transaction::sighash::signature_hash(
            &tx_data,
            &sig_input,
            &txid_parts,
        );
        assert_eq!(
            hex::encode(sighash.as_ref()),
            "3a377cd591fd60bbdf2d1075bb51b661fddeb1182c29f4ac46f5dcc459963b2a",
        );
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
        let _ = Transaction::build_transparent_bundle(&[], &vout, &[]);
    }
}
