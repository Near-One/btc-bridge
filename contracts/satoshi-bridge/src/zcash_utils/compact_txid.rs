//! Recompute a ZIP-244 txid from a *compact* commitment set instead of from the
//! full transaction bytes.
//!
//! `verify_deposit_v2` only needs two things out of `tx_bytes`: the transaction's
//! txid (to hand to the light client) and transparent output `vout` (to check the
//! amount and the deposit script). Everything else — signed transparent inputs,
//! the Sapling bundle, the Orchard bundle and its proof — is parsed only so that
//! `compute_txid()` can hash it back together. On Zcash that is pure waste: a
//! shielded-to-transparent deposit carries a ~9 KB Orchard bundle, and a large
//! consolidation carries ~148 bytes of signed input per UTXO.
//!
//! ZIP 244 makes the txid a *hash tree*, so those parts can be replaced by their
//! 32-byte subtree digests:
//!
//! ```text
//! txid  = BLAKE2b-256("ZcashTxHash_" || branch_id)
//!         ├── header_digest                                   (caller-supplied)
//!         ├── transparent_digest = BLAKE2b("ZTxIdTranspaHash")
//!         │   ├── prevouts_digest                             (caller-supplied)
//!         │   ├── sequence_digest                             (caller-supplied)
//!         │   └── outputs_digest   ← recomputed here from `vout`
//!         ├── sapling_digest                                  (caller-supplied)
//!         ├── orchard_digest                                  (caller-supplied)
//!         └── ironwood_digest      (v6 only)                  (caller-supplied)
//! ```
//!
//! Security rests on the same argument as passing the full bytes: the contract
//! derives `outputs_digest` itself from the outputs it validates, so a caller who
//! lies about *any* supplied digest — or omits/alters an output — gets a different
//! txid, and the light-client inclusion check then fails. Forging a deposit would
//! require a BLAKE2b-256 collision.
//!
//! Note that ZIP 244 txids do not commit to `script_sig` at all (signatures live
//! in the separate auth digest), which is why the inputs can collapse to
//! `prevouts_digest` + `sequence_digest`. That also means a relayer sending full
//! `tx_bytes` can blank every `script_sig` and still produce the same txid — a
//! cheaper win than this module for moderately sized transparent transactions.
//!
//! The commitment set is a *fixed* ~200 bytes (~480 as JSON) whatever the input
//! count, so it is a loss for a one-input transparent deposit and a large win
//! for anything shielded or wide. See `tests/test_compact_deposit.rs`.

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

/// TxId tree root personalization prefix (ZIP 244).
const ZCASH_TX_PERSONALIZATION_PREFIX: &[u8; 12] = b"ZcashTxHash_";
const ZCASH_TRANSPARENT_HASH_PERSONALIZATION: &[u8; 16] = b"ZTxIdTranspaHash";
const ZCASH_OUTPUTS_HASH_PERSONALIZATION: &[u8; 16] = b"ZTxIdOutputsHash";
const ZCASH_SAPLING_HASH_PERSONALIZATION: &[u8; 16] = b"ZTxIdSaplingHash";

/// v5 header word (`overwintered` bit | version 5).
pub const V5_TX_VERSION_HEADER: u32 = 0x8000_0005;
/// v6 header word (`overwintered` bit | version 6).
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

/// The subtree digests that stand in for the parts of the transaction the bridge
/// does not need to inspect. All values are the raw 32-byte ZIP-244 node digests;
/// for an absent bundle that is the protocol-defined *empty bundle* digest, which
/// the caller computes off-chain (see [`empty_sapling_digest`]).
pub struct CompactTxidParts<'a> {
    /// `0x80000005` or `0x80000006` — selects the txid tree arity (v6 adds the
    /// Ironwood slot). Its value is also committed to by `header_digest`.
    pub version_header: u32,
    /// Consensus branch ID, which personalizes the tree root.
    pub consensus_branch_id: u32,
    /// ZIP 244 T.1: version, version group id, branch id, lock time, expiry height.
    pub header_digest: [u8; 32],
    /// ZIP 244 T.2a: every input's `(txid, index)`.
    pub prevouts_digest: [u8; 32],
    /// ZIP 244 T.2b: every input's `nSequence`.
    pub sequence_digest: [u8; 32],
    /// ZIP 244 T.3.
    pub sapling_digest: [u8; 32],
    /// ZIP 244 T.4.
    pub orchard_digest: [u8; 32],
    /// ZIP 229 Ironwood slot; required for (and only used by) v6.
    pub ironwood_digest: Option<[u8; 32]>,
    /// The complete list of transparent outputs, in order. Required in full: the
    /// digest is taken over all of them, so a missing or reordered output changes
    /// the txid.
    pub vout: &'a [TxOut],
}

/// ZIP 244 T.2c — the digest of all transparent outputs, in Zcash's canonical
/// `TxOut` encoding (`value` as LE u64, then CompactSize-prefixed script).
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

/// The ZIP 244 T.3 digest of an absent Sapling bundle. Most deposits have no
/// Sapling bundle, so this is what `sapling_digest` normally carries.
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

/// The ZIP 244 T.4 digest of an absent Orchard bundle, for a v5 or v6
/// transaction (the personalization differs between the two).
pub fn empty_orchard_digest(version_header: u32) -> [u8; 32] {
    empty_bundle_digest(ValuePool::Orchard, orchard_tx_version(version_header))
}

/// The ZIP 229 digest of an absent Ironwood bundle. Only v6 transactions carry
/// this slot.
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

/// Recompute the transaction's ZIP-244 txid from `parts`.
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

    // ZIP 244 txids are displayed byte-reversed, which is what `Txid` does.
    Txid::from_byte_array(hash32(&personal, &nodes))
}

/// A ZIP-244 compact commitment set: everything needed to recompute a
/// transaction's txid and inspect its transparent outputs, without shipping the
/// transaction itself. Passed to `verify_deposit_v2` as the `tx_bytes` argument
/// — a JSON object where the full bytes would be a base64 string (see
/// [`crate::DepositTxProof`]).
///
/// Every digest is the raw 32-byte ZIP-244 node digest as produced by
/// `zcash_primitives::transaction::txid::TxIdDigester` — **not** byte-reversed
/// the way txids are displayed. Where a bundle is absent, pass the
/// protocol-defined empty-bundle digest for that pool.
#[near(serializers = [json])]
pub struct CompactTxProof {
    /// `0x80000005` (v5) or `0x80000006` (v6).
    pub version_header: u32,
    /// Consensus branch ID of the epoch the transaction was mined in.
    pub consensus_branch_id: u32,
    /// ZIP 244 T.1 — header digest.
    pub header_digest: Base64VecU8,
    /// ZIP 244 T.2a — digest of every input's `(txid, index)`.
    pub prevouts_digest: Base64VecU8,
    /// ZIP 244 T.2b — digest of every input's `nSequence`.
    pub sequence_digest: Base64VecU8,
    /// ZIP 244 T.3 — Sapling bundle digest.
    pub sapling_digest: Base64VecU8,
    /// ZIP 244 T.4 — Orchard bundle digest.
    pub orchard_digest: Base64VecU8,
    /// ZIP 229 Ironwood bundle digest. Required for v6, rejected for v5.
    pub ironwood_digest: Option<Base64VecU8>,
    /// The complete transparent output list, in order. Required in full, because
    /// the txid commits to a digest over all of them.
    pub outputs: Vec<TxOut>,
}

fn digest32(bytes: &Base64VecU8, field: &str) -> [u8; 32] {
    <[u8; 32]>::try_from(bytes.0.as_slice())
        .unwrap_or_else(|_| env::panic_str(&format!("{field} must be exactly 32 bytes")))
}

impl CompactTxProof {
    /// Recompute the txid this proof commits to. Any mismatch between the
    /// supplied digests/outputs and the real transaction yields a different
    /// txid, which the light-client inclusion check then rejects.
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
    use zcash_primitives::transaction::txid::TxIdDigester;

    /// A real mainnet v5 shielded-to-transparent transaction: zero transparent
    /// inputs, one transparent output, and a ~9 KB Orchard bundle. Same vector as
    /// `btc_pending_info::tests::test_zcash_tx_bytes`.
    fn real_tx() -> Transaction {
        let hex = include_str!("../../tests/data/zcash_shielded_deposit_tx.hex");
        let bytes = hex::decode(hex.trim()).unwrap();
        Transaction::decode(&bytes, &crate::network::Chain::ZcashMainnet).unwrap()
    }

    fn to_array(hash: &blake2b_simd::Hash) -> [u8; 32] {
        hash.as_bytes().try_into().unwrap()
    }

    /// Rebuild the fixture with one transparent input carrying `script_sig`.
    /// The fixture has `tx_in_count == 0` at byte 20, right after the five
    /// header words, so the input list can be spliced in there.
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

    /// Build the JSON a relayer would send as `tx_bytes`, from the raw
    /// transaction. This mirrors what the off-chain side must do with
    /// librustzcash: take the four (or five) level-1 digests plus the two
    /// transparent input digests straight out of `TxIdDigester`.
    fn compact_proof_json(tx: &Transaction) -> near_sdk::serde_json::Value {
        let digests = tx.inner_tx.digest(TxIdDigester);
        let transparent = digests.transparent_digests.as_ref().unwrap();
        let b64 = |h: &blake2b_simd::Hash| Base64VecU8(h.as_bytes().to_vec());

        near_sdk::serde_json::json!({
            "version_header": V5_TX_VERSION_HEADER,
            "consensus_branch_id": u32::from(tx.inner_tx.consensus_branch_id()),
            "header_digest": b64(&digests.header_digest),
            "prevouts_digest": b64(&transparent.prevouts_digest),
            "sequence_digest": b64(&transparent.sequence_digest),
            "sapling_digest": digests
                .sapling_digest
                .as_ref()
                .map_or_else(|| Base64VecU8(empty_sapling_digest().to_vec()), b64),
            "orchard_digest": b64(digests.orchard_digest.as_ref().unwrap()),
            "outputs": tx.output(),
            // `ironwood_digest` deliberately omitted: a missing Option field must
            // deserialize to None, which is what keeps `tx_bytes` optional on the
            // public API without breaking existing callers.
        })
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

    /// ZIP 244 moved all signature data out of the txid tree, so blanking every
    /// `script_sig` leaves the txid untouched. That makes a signature-stripped
    /// transaction an equally valid deposit proof today, with no contract change —
    /// worth ~110 bytes per P2PKH input.
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
        let expected = tx.compute_txid();

        // Everything the relayer would precompute off-chain from the raw tx.
        let digests = tx.inner_tx.digest(TxIdDigester);
        let transparent = digests.transparent_digests.as_ref().unwrap();
        let parts_vout = tx.output();

        let recomputed = compact_txid(&CompactTxidParts {
            version_header: V5_TX_VERSION_HEADER,
            consensus_branch_id: u32::from(tx.inner_tx.consensus_branch_id()),
            header_digest: to_array(&digests.header_digest),
            prevouts_digest: to_array(&transparent.prevouts_digest),
            sequence_digest: to_array(&transparent.sequence_digest),
            sapling_digest: digests
                .sapling_digest
                .as_ref()
                .map_or_else(empty_sapling_digest, to_array),
            orchard_digest: to_array(digests.orchard_digest.as_ref().unwrap()),
            ironwood_digest: None,
            vout: &parts_vout,
        });

        assert_eq!(recomputed, expected);
    }

    #[test]
    fn compact_txid_rejects_a_tampered_output() {
        let tx = real_tx();
        let digests = tx.inner_tx.digest(TxIdDigester);
        let transparent = digests.transparent_digests.as_ref().unwrap();

        let mut tampered = tx.output();
        tampered[0].value += bitcoin::Amount::from_sat(1);

        let recomputed = compact_txid(&CompactTxidParts {
            version_header: V5_TX_VERSION_HEADER,
            consensus_branch_id: u32::from(tx.inner_tx.consensus_branch_id()),
            header_digest: to_array(&digests.header_digest),
            prevouts_digest: to_array(&transparent.prevouts_digest),
            sequence_digest: to_array(&transparent.sequence_digest),
            sapling_digest: digests
                .sapling_digest
                .as_ref()
                .map_or_else(empty_sapling_digest, to_array),
            orchard_digest: to_array(digests.orchard_digest.as_ref().unwrap()),
            ironwood_digest: None,
            vout: &tampered,
        });

        assert_ne!(recomputed, tx.compute_txid());
    }

    /// The whole point: the compact commitment set is a constant ~200 bytes where
    /// the raw transaction is kilobytes.
    #[test]
    fn compact_proof_is_far_smaller_than_tx_bytes() {
        let tx = real_tx();
        let full = tx.encode().unwrap().len();
        // 4 + 4 header words, 5 * 32 digest bytes, plus the serialized outputs.
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
}
