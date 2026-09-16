//! Recompute a ZIP-244 txid from a compact commitment set instead of from the
//! full transaction bytes.
//!
//! The deposit path needs only the txid and the transparent outputs. ZIP 244
//! makes the txid a hash tree, so the parts the bridge never inspects (inputs,
//! Sapling, Orchard, Ironwood) can be supplied as their 32-byte subtree digests:
//! a fixed ~200 bytes whatever the transaction size.
//!
//! As trustless as passing the full bytes: the contract derives the outputs
//! digest itself from the outputs it validates, so a lie about any supplied
//! digest or output changes the txid and fails the inclusion check.

use bitcoin::hashes::Hash as _;
use bitcoin::{TxOut, Txid};
use blake2b_simd::Params;
use near_sdk::json_types::Base64VecU8;
use near_sdk::{env, near, require};
use orchard::bundle::TxVersion as OrchardTxVersion;
use orchard::ValuePool;
use zcash_protocol::value::Zatoshis;
use zcash_script::script::Code;
use zcash_transparent::address::Script;
use zcash_transparent::bundle::TxOut as ZcashTxOut;

use crate::DepositTxSummary;

const ZCASH_TX_PERSONALIZATION_PREFIX: &[u8; 12] = b"ZcashTxHash_";
const ZCASH_TRANSPARENT_HASH_PERSONALIZATION: &[u8; 16] = b"ZTxIdTranspaHash";
const ZCASH_OUTPUTS_HASH_PERSONALIZATION: &[u8; 16] = b"ZTxIdOutputsHash";
const ZCASH_SAPLING_HASH_PERSONALIZATION: &[u8; 16] = b"ZTxIdSaplingHash";

pub const V5_TX_VERSION_HEADER: u32 = 0x8000_0005;
pub const V6_TX_VERSION_HEADER: u32 = 0x8000_0006;

fn hash32(personal: &[u8; 16], parts: &[&[u8]]) -> [u8; 32] {
    let mut state = Params::new().hash_length(32).personal(personal).to_state();
    for part in parts {
        state.update(part);
    }
    let hash = state.finalize();
    hash.as_bytes()
        .try_into()
        .expect("BLAKE2b-256 produces 32 bytes")
}

/// Raw 32-byte ZIP-244 subtree digests standing in for the parts of the
/// transaction the bridge does not inspect, plus the complete output list.
pub struct CompactTxidParts<'a> {
    pub version_header: u32,
    pub consensus_branch_id: u32,
    pub header_digest: [u8; 32],
    pub prevouts_digest: [u8; 32],
    pub sequence_digest: [u8; 32],
    pub sapling_digest: [u8; 32],
    pub orchard_digest: [u8; 32],
    pub ironwood_digest: Option<[u8; 32]>,
    pub vout: &'a [TxOut],
}

/// ZIP 244 T.2c — the digest of all transparent outputs.
pub fn outputs_digest(vout: &[TxOut]) -> [u8; 32] {
    let mut buf = Vec::new();
    for out in vout {
        let value = Zatoshis::from_u64(out.value.to_sat())
            .unwrap_or_else(|_| env::panic_str("Output value out of range"));
        let script = Script(Code(out.script_pubkey.to_bytes()));
        ZcashTxOut::new(value, script)
            .write(&mut buf)
            .unwrap_or_else(|_| env::panic_str("Failed to serialize output"));
    }
    hash32(ZCASH_OUTPUTS_HASH_PERSONALIZATION, &[&buf])
}

/// ZIP 244 T.3 digest of an absent Sapling bundle.
pub fn empty_sapling_digest() -> [u8; 32] {
    hash32(ZCASH_SAPLING_HASH_PERSONALIZATION, &[])
}

fn empty_bundle_digest(value_pool: ValuePool, tx_version: OrchardTxVersion) -> [u8; 32] {
    let hash = orchard::bundle::commitments::hash_bundle_txid_empty(value_pool, tx_version)
        .unwrap_or_else(|_| env::panic_str("Invalid empty bundle commitment domain"));
    hash.as_bytes()
        .try_into()
        .expect("BLAKE2b-256 produces 32 bytes")
}

/// ZIP 244 T.4 digest of an absent Orchard bundle; personalization differs
/// between v5 and v6.
pub fn empty_orchard_digest(version_header: u32) -> [u8; 32] {
    empty_bundle_digest(ValuePool::Orchard, orchard_tx_version(version_header))
}

/// ZIP 229 digest of an absent Ironwood bundle (v6 only).
pub fn empty_ironwood_digest() -> [u8; 32] {
    empty_bundle_digest(ValuePool::Ironwood, OrchardTxVersion::V6)
}

fn orchard_tx_version(version_header: u32) -> OrchardTxVersion {
    match version_header {
        V5_TX_VERSION_HEADER => OrchardTxVersion::V5,
        V6_TX_VERSION_HEADER => OrchardTxVersion::V6,
        _ => env::panic_str("Unsupported transaction version"),
    }
}

pub fn compact_txid(parts: &CompactTxidParts) -> Txid {
    require!(
        !parts.vout.is_empty(),
        "Compact txid requires a transparent output"
    );

    let transparent_digest = hash32(
        ZCASH_TRANSPARENT_HASH_PERSONALIZATION,
        &[
            &parts.prevouts_digest,
            &parts.sequence_digest,
            &outputs_digest(parts.vout),
        ],
    );

    let mut personal = [0u8; 16];
    personal[..12].copy_from_slice(ZCASH_TX_PERSONALIZATION_PREFIX);
    personal[12..].copy_from_slice(&parts.consensus_branch_id.to_le_bytes());

    let mut nodes: Vec<&[u8]> = vec![
        &parts.header_digest,
        &transparent_digest,
        &parts.sapling_digest,
        &parts.orchard_digest,
    ];
    let ironwood_digest = match parts.version_header {
        V5_TX_VERSION_HEADER => {
            require!(
                parts.ironwood_digest.is_none(),
                "v5 transactions have no Ironwood bundle"
            );
            None
        }
        V6_TX_VERSION_HEADER => Some(
            parts
                .ironwood_digest
                .unwrap_or_else(|| env::panic_str("v6 transactions require an Ironwood digest")),
        ),
        _ => env::panic_str("Unsupported transaction version"),
    };
    if let Some(digest) = ironwood_digest.as_ref() {
        nodes.push(digest);
    }

    Txid::from_byte_array(hash32(&personal, &nodes))
}

/// The `tx_bytes` argument of `verify_deposit_v2` in its compact form: a JSON
/// object where the full bytes would be a base64 string (see
/// [`crate::DepositTxProof`]).
///
/// Digests are raw 32-byte ZIP-244 node digests as produced by
/// `zcash_primitives::transaction::txid::TxIdDigester` — **not** byte-reversed
/// the way txids are displayed; where a bundle is absent, pass the
/// protocol-defined empty-bundle digest for that pool. `outputs` must be the
/// complete list, in order.
#[near(serializers = [json])]
pub struct CompactTxProof {
    /// `0x80000005` (v5) or `0x80000006` (v6).
    pub version_header: u32,
    pub consensus_branch_id: u32,
    pub header_digest: Base64VecU8,
    pub prevouts_digest: Base64VecU8,
    pub sequence_digest: Base64VecU8,
    pub sapling_digest: Base64VecU8,
    pub orchard_digest: Base64VecU8,
    /// Required for v6, rejected for v5.
    pub ironwood_digest: Option<Base64VecU8>,
    pub outputs: Vec<TxOut>,
}

fn digest32(bytes: &Base64VecU8, field: &str) -> [u8; 32] {
    <[u8; 32]>::try_from(bytes.0.as_slice())
        .unwrap_or_else(|_| env::panic_str(&format!("{field} must be exactly 32 bytes")))
}

impl CompactTxProof {
    pub fn resolve(self) -> DepositTxSummary {
        let tx_id = compact_txid(&CompactTxidParts {
            version_header: self.version_header,
            consensus_branch_id: self.consensus_branch_id,
            header_digest: digest32(&self.header_digest, "header_digest"),
            prevouts_digest: digest32(&self.prevouts_digest, "prevouts_digest"),
            sequence_digest: digest32(&self.sequence_digest, "sequence_digest"),
            sapling_digest: digest32(&self.sapling_digest, "sapling_digest"),
            orchard_digest: digest32(&self.orchard_digest, "orchard_digest"),
            ironwood_digest: self
                .ironwood_digest
                .as_ref()
                .map(|d| digest32(d, "ironwood_digest")),
            vout: &self.outputs,
        });

        DepositTxSummary {
            tx_id,
            outputs: self.outputs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zcash_utils::transaction::Transaction;
    use zcash_primitives::transaction::txid::{to_txid, TxIdDigester};

    /// Real mainnet **v5** shielded-to-transparent tx: no transparent inputs, one
    /// output, ~9 KB Orchard bundle.
    const V5_FIXTURE_HEX: &str = include_str!("../../tests/data/zcash_shielded_deposit_tx.hex");

    /// Real **v6** (NU6.3) shielded-to-transparent tx: no Sapling and no Orchard
    /// bundle, 1.67 MB Ironwood bundle — the only fixture reaching the v6 half of
    /// the tree. Raw bytes rather than hex, which would double it in the repo.
    const V6_FIXTURE: &[u8] =
        include_bytes!("../../tests/data/zcash_shielded_deposit_tx_large.bin");

    /// The one assertion here not derived from `zcash_primitives`, so a behaviour
    /// change there cannot move both sides together unnoticed.
    const V6_FIXTURE_TXID: &str =
        "9de2c62bd6dd6c408fb4d04c329f73509f629b9233330c9cedaaecece11076e4";

    fn decode_fixture(bytes: &[u8]) -> Transaction {
        Transaction::decode(bytes, &crate::network::Chain::ZcashMainnet).unwrap()
    }

    fn real_tx() -> Transaction {
        decode_fixture(&hex::decode(V5_FIXTURE_HEX.trim()).unwrap())
    }

    fn large_v6_tx() -> Transaction {
        decode_fixture(V6_FIXTURE)
    }

    fn to_array(hash: &blake2b_simd::Hash) -> [u8; 32] {
        hash.as_bytes().try_into().unwrap()
    }

    fn version_header_of(tx: &Transaction) -> u32 {
        tx.inner_tx.version().header()
    }

    /// Everything a relayer precomputes off-chain for `tx`. `vout` is taken
    /// separately so a test can tamper with the outputs, leaving the digests honest.
    fn parts_for<'a>(tx: &Transaction, vout: &'a [TxOut]) -> CompactTxidParts<'a> {
        let digests = tx.inner_tx.digest(TxIdDigester);
        let transparent = digests
            .transparent_digests
            .as_ref()
            .expect("fixture has a transparent bundle");
        let version_header = version_header_of(tx);

        CompactTxidParts {
            version_header,
            consensus_branch_id: u32::from(tx.inner_tx.consensus_branch_id()),
            header_digest: to_array(&digests.header_digest),
            prevouts_digest: to_array(&transparent.prevouts_digest),
            sequence_digest: to_array(&transparent.sequence_digest),
            sapling_digest: digests
                .sapling_digest
                .as_ref()
                .map_or_else(empty_sapling_digest, to_array),
            orchard_digest: digests
                .orchard_digest
                .as_ref()
                .map_or_else(|| empty_orchard_digest(version_header), to_array),
            ironwood_digest: (version_header == V6_TX_VERSION_HEADER).then(|| {
                digests
                    .ironwood_digest
                    .as_ref()
                    .map_or_else(empty_ironwood_digest, to_array)
            }),
            vout,
        }
    }

    /// Splice one transparent input carrying `script_sig` into the fixture, at
    /// byte 20, where `tx_in_count == 0` sits right after the five header words.
    fn fixture_with_one_input(script_sig: &[u8]) -> Vec<u8> {
        let hex = include_str!("../../tests/data/zcash_shielded_deposit_tx.hex");
        let bytes = hex::decode(hex.trim()).unwrap();
        assert_eq!(bytes[20], 0, "fixture should have no transparent inputs");

        let mut vin = vec![1u8]; // tx_in_count
        vin.extend_from_slice(&[0x42u8; 32]); // prevout txid
        vin.extend_from_slice(&7u32.to_le_bytes()); // prevout index
        assert!(script_sig.len() < 253);
        vin.push(script_sig.len() as u8);
        vin.extend_from_slice(script_sig);
        vin.extend_from_slice(&0xffff_fffeu32.to_le_bytes()); // nSequence

        let mut out = bytes[..20].to_vec();
        out.extend_from_slice(&vin);
        out.extend_from_slice(&bytes[21..]);
        out
    }

    /// Build the JSON a relayer would send as `tx_bytes`.
    fn compact_proof_json(tx: &Transaction) -> near_sdk::serde_json::Value {
        let outputs = tx.output();
        let parts = parts_for(tx, &outputs);
        let b64 = |d: [u8; 32]| Base64VecU8(d.to_vec());

        let mut json = near_sdk::serde_json::json!({
            "version_header": parts.version_header,
            "consensus_branch_id": parts.consensus_branch_id,
            "header_digest": b64(parts.header_digest),
            "prevouts_digest": b64(parts.prevouts_digest),
            "sequence_digest": b64(parts.sequence_digest),
            "sapling_digest": b64(parts.sapling_digest),
            "orchard_digest": b64(parts.orchard_digest),
            "outputs": outputs,
        });
        // v5 leaves the field out entirely rather than sending null.
        if let Some(ironwood) = parts.ironwood_digest {
            json["ironwood_digest"] = near_sdk::serde_json::to_value(b64(ironwood)).unwrap();
        }
        json
    }

    #[test]
    fn compact_tx_proof_round_trips_through_json() {
        let tx = real_tx();
        let proof: CompactTxProof =
            near_sdk::serde_json::from_value(compact_proof_json(&tx)).unwrap();

        let summary = proof.resolve();
        assert_eq!(summary.tx_id, tx.compute_txid());
        assert_eq!(summary.outputs, tx.output());
    }

    #[test]
    #[should_panic(expected = "orchard_digest must be exactly 32 bytes")]
    fn compact_tx_proof_rejects_a_short_digest() {
        let tx = real_tx();
        let mut json = compact_proof_json(&tx);
        json["orchard_digest"] =
            near_sdk::serde_json::to_value(Base64VecU8(vec![0u8; 31])).unwrap();

        let proof: CompactTxProof = near_sdk::serde_json::from_value(json).unwrap();
        proof.resolve();
    }

    /// ZIP 244 moved signature data out of the txid tree, so a signature-stripped
    /// transaction is an equally valid deposit proof with no contract change.
    #[test]
    fn txid_ignores_script_sig() {
        let mut script_sig = vec![0x47]; // push 71-byte DER signature
        script_sig.extend_from_slice(&[0xab; 71]);
        script_sig.push(0x21); // push 33-byte compressed pubkey
        script_sig.extend_from_slice(&[0xcd; 33]);

        let signed = fixture_with_one_input(&script_sig);
        let stripped = fixture_with_one_input(&[]);
        assert_eq!(signed.len() - stripped.len(), script_sig.len());

        let chain = crate::network::Chain::ZcashMainnet;
        assert_eq!(
            Transaction::decode(&signed, &chain).unwrap().compute_txid(),
            Transaction::decode(&stripped, &chain)
                .unwrap()
                .compute_txid(),
        );
    }

    #[test]
    fn compact_txid_matches_full_transaction() {
        let tx = real_tx();
        assert_eq!(version_header_of(&tx), V5_TX_VERSION_HEADER);

        let outputs = tx.output();
        assert_eq!(compact_txid(&parts_for(&tx, &outputs)), tx.compute_txid());
    }

    /// The 5-node v6 root: Ironwood slot populated, Orchard slot filled by the
    /// v6 empty-bundle substitution.
    #[test]
    fn compact_txid_matches_large_v6_ironwood_transaction() {
        let tx = large_v6_tx();
        assert_eq!(version_header_of(&tx), V6_TX_VERSION_HEADER);
        assert!(
            tx.inner_tx.orchard_bundle().is_none() && tx.inner_tx.sapling_bundle().is_none(),
            "fixture should exercise the empty Orchard and Sapling substitutions"
        );

        let outputs = tx.output();
        let parts = parts_for(&tx, &outputs);
        assert!(
            parts.ironwood_digest.is_some(),
            "fixture should carry a real Ironwood bundle digest"
        );

        let recomputed = compact_txid(&parts);
        assert_eq!(recomputed, tx.compute_txid());
        assert_eq!(recomputed.to_string(), V6_FIXTURE_TXID);
    }

    /// Differential test against the reference implementation for both versions.
    /// `to_txid` supplies its own empty-bundle digests for the `None` slots, so
    /// agreement also pins `empty_sapling_digest` and `empty_orchard_digest`.
    #[test]
    fn compact_txid_matches_reference_to_txid() {
        for tx in [real_tx(), large_v6_tx()] {
            let outputs = tx.output();
            let reference = to_txid(
                tx.inner_tx.version(),
                tx.inner_tx.consensus_branch_id(),
                &tx.inner_tx.digest(TxIdDigester),
            );
            assert_eq!(
                compact_txid(&parts_for(&tx, &outputs)).as_byte_array(),
                reference.as_ref(),
                "divergence from reference to_txid for version {:?}",
                tx.inner_tx.version()
            );
        }
    }

    /// The one empty-bundle substitution no fixture reaches, so pin it against the
    /// reference directly.
    #[test]
    fn empty_ironwood_digest_matches_reference_substitution() {
        let tx = large_v6_tx();
        let outputs = tx.output();

        let mut digests = tx.inner_tx.digest(TxIdDigester);
        digests.ironwood_digest = None;
        let reference = to_txid(
            tx.inner_tx.version(),
            tx.inner_tx.consensus_branch_id(),
            &digests,
        );

        let mut parts = parts_for(&tx, &outputs);
        parts.ironwood_digest = Some(empty_ironwood_digest());

        assert_eq!(compact_txid(&parts).as_byte_array(), reference.as_ref());
    }

    #[test]
    fn compact_txid_rejects_a_tampered_output() {
        let tx = real_tx();
        let mut tampered = tx.output();
        tampered[0].value += bitcoin::Amount::from_sat(1);

        assert_ne!(compact_txid(&parts_for(&tx, &tampered)), tx.compute_txid());
    }

    #[test]
    fn compact_txid_binds_the_ironwood_digest() {
        let tx = large_v6_tx();
        let outputs = tx.output();

        let mut parts = parts_for(&tx, &outputs);
        parts.ironwood_digest = Some([0xaa; 32]);

        assert_ne!(compact_txid(&parts), tx.compute_txid());
    }

    #[test]
    fn compact_tx_proof_round_trips_through_json_for_v6() {
        let tx = large_v6_tx();
        let json = compact_proof_json(&tx);
        assert!(
            json.get("ironwood_digest").is_some(),
            "the v6 wire form must carry ironwood_digest"
        );

        let proof: CompactTxProof = near_sdk::serde_json::from_value(json).unwrap();
        let summary = proof.resolve();
        assert_eq!(summary.tx_id, tx.compute_txid());
        assert_eq!(summary.outputs, tx.output());
    }

    #[test]
    #[should_panic(expected = "v5 transactions have no Ironwood bundle")]
    fn compact_txid_rejects_an_ironwood_digest_on_v5() {
        let tx = real_tx();
        let outputs = tx.output();

        let mut parts = parts_for(&tx, &outputs);
        parts.ironwood_digest = Some([0xaa; 32]);

        compact_txid(&parts);
    }

    #[test]
    fn compact_proof_is_far_smaller_than_tx_bytes() {
        let tx = real_tx();
        let full = tx.encode().unwrap().len();
        // Two header words, five digests, plus the serialized outputs.
        let compact = 8
            + 5 * 32
            + tx.output()
                .iter()
                .map(|o| 8 + 1 + o.script_pubkey.len())
                .sum::<usize>();
        assert!(
            full > 9_000,
            "fixture should be a large shielded tx: {full}"
        );
        assert!(compact < 250, "compact form should be tiny: {compact}");
    }

    /// The case that motivates the feature: base64-encoded this transaction is
    /// past NEAR's per-transaction ceiling, so the compact form is not an
    /// optimisation but the only way to credit the deposit.
    #[test]
    fn large_v6_deposit_cannot_be_proven_with_full_tx_bytes() {
        const NEAR_MAX_TX_SIZE: usize = 1_572_864;

        let tx = large_v6_tx();
        let raw = tx.encode().unwrap().len();
        let base64_len = raw.div_ceil(3) * 4;

        let outputs = tx.output();
        let compact = 8
            + 5 * 32
            + outputs
                .iter()
                .map(|o| 8 + 1 + o.script_pubkey.len())
                .sum::<usize>();

        assert!(
            base64_len > NEAR_MAX_TX_SIZE,
            "fixture should be unsubmittable as tx_bytes: {base64_len} base64 bytes \
             from {raw} raw vs NEAR's {NEAR_MAX_TX_SIZE} limit"
        );
        assert!(compact < 250, "compact form should be tiny: {compact}");
    }
}
