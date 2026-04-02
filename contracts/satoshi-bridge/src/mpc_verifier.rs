//! MPC Foreign Chain Transaction Verification for DASH.
//!
//! Replaces the BTC light client verification used by Bitcoin with the NEAR MPC
//! network's `verify_foreign_transaction` endpoint. The MPC nodes query a DASH
//! RPC node directly to confirm transaction existence and confirmations, then
//! threshold-sign an attestation.
//!
//! This module follows the same pattern as `mpc-omni-prover` from the omni-bridge,
//! but inlined directly into the bridge contract.
//!
//! ## Inlined Types
//!
//! The types below are minimal local copies of types from `near-mpc-contract-interface`.
//! They are only used to serialize JSON arguments for the `verify_foreign_transaction`
//! cross-contract call. We inline them to avoid pulling in `near-mpc-sdk` which
//! requires a newer Rust toolchain than the project currently uses.
//!
//! Source: <https://github.com/near/mpc/blob/main/crates/near-mpc-contract-interface/src/types/foreign_chain.rs>

use crate::{ext_contract, require, Contract, Gas, Promise};
use near_sdk::serde::{Serialize, Serializer};
use near_sdk::NearToken;

// ---------------------------------------------------------------------------
// Inlined MPC contract types (serialization-only, matching the MPC contract's
// JSON interface exactly)
// ---------------------------------------------------------------------------

/// 32-byte Bitcoin/DASH transaction ID. Serialized as a hex string to match
/// the MPC contract's `#[serde_as(as = "Hex")]` on `BitcoinTxId`.
#[derive(Clone)]
pub struct BitcoinTxId(pub [u8; 32]);

impl Serialize for BitcoinTxId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

/// Block confirmation count. Serialized as a plain `u64` (newtype transparency).
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct BlockConfirmations(pub u64);

/// Bitcoin extractor enum. Serialized as an externally-tagged JSON enum,
/// matching the MPC contract's `BitcoinExtractor`.
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum BitcoinExtractor {
    BlockHash,
}

/// Bitcoin RPC request parameters.
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct BitcoinRpcRequest {
    pub tx_id: BitcoinTxId,
    pub confirmations: BlockConfirmations,
    pub extractors: Vec<BitcoinExtractor>,
}

/// Foreign chain RPC request. Externally-tagged enum matching the MPC contract's
/// `ForeignChainRpcRequest` type.
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum ForeignChainRpcRequest {
    Bitcoin(BitcoinRpcRequest),
}

/// Payload version for foreign transaction verification.
/// The MPC contract serializes this as a `u8` via `serde_repr` (V1 = 1).
pub struct ForeignTxPayloadVersion;

impl Serialize for ForeignTxPayloadVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

/// Domain ID for the MPC contract. Serialized as a plain `u64` (newtype transparency).
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct DomainId(pub u64);

/// Arguments for `verify_foreign_transaction` on the MPC contract.
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct VerifyForeignTransactionRequestArgs {
    pub request: ForeignChainRpcRequest,
    pub domain_id: DomainId,
    pub payload_version: ForeignTxPayloadVersion,
}

// ---------------------------------------------------------------------------
// MPC contract ext_contract and bridge integration
// ---------------------------------------------------------------------------

/// Domain ID for foreign transaction verification.
/// Must match the domain configured in the MPC contract for this bridge.
///
/// The MPC signer contract exposes both `sign()` and `verify_foreign_transaction()`
/// on the same account, so we reuse `chain_signatures_account_id` from Config.
pub const FOREIGN_TX_DOMAIN_ID: u64 = 3;

pub const GAS_FOR_VERIFY_FOREIGN_TX: Gas = Gas::from_tgas(15);

const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

#[ext_contract(ext_mpc_contract)]
pub trait MpcContract {
    fn verify_foreign_transaction(&mut self, request: VerifyForeignTransactionRequestArgs);
}

/// Convert a hex-encoded transaction ID string to a `BitcoinTxId`.
/// Bitcoin/DASH txids are displayed in reversed byte order, so we reverse the bytes.
pub fn tx_id_hex_to_bytes(tx_id: &str) -> BitcoinTxId {
    let mut bytes = hex::decode(tx_id).expect("ERR_INVALID_TX_ID: not valid hex");
    require!(bytes.len() == 32, "ERR_INVALID_TX_ID: must be 32 bytes");
    bytes.reverse(); // Bitcoin/DASH txids are displayed in reversed byte order
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    BitcoinTxId(arr)
}

impl Contract {
    /// Call the MPC contract to verify that a transaction exists on the DASH
    /// network with the required number of confirmations.
    ///
    /// This replaces `verify_transaction_inclusion_promise()` from btc_light_client.
    /// The MPC nodes query `getrawtransaction` on a DASH RPC node (same API as
    /// Bitcoin since DASH is a Bitcoin fork), check confirmations >= threshold,
    /// and return a signed attestation.
    ///
    /// The `verify_foreign_transaction` call on the MPC contract will **fail**
    /// (revert) if the transaction doesn't exist or doesn't have enough
    /// confirmations. A successful callback means the tx was verified.
    ///
    /// We use `ForeignChainRpcRequest::Bitcoin` because DASH exposes the exact
    /// same `getrawtransaction` RPC interface as Bitcoin. The MPC contract's
    /// `BitcoinInspector` works identically for DASH — it's just pointed at a
    /// DASH RPC provider via the foreign chain policy configuration.
    pub fn verify_transaction_via_mpc(&self, tx_id: String, confirmations: u64) -> Promise {
        let config = self.internal_config();

        let request_args = VerifyForeignTransactionRequestArgs {
            request: ForeignChainRpcRequest::Bitcoin(BitcoinRpcRequest {
                tx_id: tx_id_hex_to_bytes(&tx_id),
                confirmations: BlockConfirmations(confirmations),
                extractors: vec![BitcoinExtractor::BlockHash],
            }),
            domain_id: DomainId(FOREIGN_TX_DOMAIN_ID),
            payload_version: ForeignTxPayloadVersion,
        };

        ext_mpc_contract::ext(config.chain_signatures_account_id.clone())
            .with_static_gas(GAS_FOR_VERIFY_FOREIGN_TX)
            .with_attached_deposit(ONE_YOCTO)
            .verify_foreign_transaction(request_args)
    }
}
