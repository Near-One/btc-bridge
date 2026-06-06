// Minimal local copies of types from `near-mpc-contract-interface`.
// Inlined to avoid pulling in `near-mpc-sdk` which requires Rust 1.88+.
// Source: https://github.com/near/mpc/blob/main/crates/near-mpc-contract-interface/src/types/foreign_chain.rs

use crate::{ext_contract, require, Contract, Gas, Promise};
use near_sdk::serde::{Deserialize, Serialize, Serializer};
use near_sdk::NearToken;

#[derive(Clone)]
pub struct BitcoinTxId(pub [u8; 32]);

impl Serialize for BitcoinTxId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct BlockConfirmations(pub u64);

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum BitcoinExtractor {
    BlockHash,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct BitcoinRpcRequest {
    pub tx_id: BitcoinTxId,
    pub confirmations: BlockConfirmations,
    pub extractors: Vec<BitcoinExtractor>,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub enum ForeignChainRpcRequest {
    Bitcoin(BitcoinRpcRequest),
}

// Serialized as u8 via serde_repr in the MPC contract (V1 = 1).
pub struct ForeignTxPayloadVersion;

impl Serialize for ForeignTxPayloadVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct DomainId(pub u64);

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct VerifyForeignTransactionRequestArgs {
    pub request: ForeignChainRpcRequest,
    pub domain_id: DomainId,
    pub payload_version: ForeignTxPayloadVersion,
}

/// Minimal deserialization of the MPC `verify_foreign_transaction` response.
/// Only `payload_hash` is checked; the `signature` field is ignored via `deny_unknown_fields = false` (default).
#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct VerifyForeignTransactionResponse {
    /// Hex-encoded Hash256 of the signed payload.
    pub payload_hash: String,
}

// Must match the domain configured in the MPC contract for this bridge.
pub const FOREIGN_TX_DOMAIN_ID: u64 = 3;

pub const GAS_FOR_VERIFY_FOREIGN_TX: Gas = Gas::from_tgas(15);

const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);

#[ext_contract(ext_mpc_contract)]
pub trait MpcContract {
    fn verify_foreign_transaction(&mut self, request: VerifyForeignTransactionRequestArgs);
}

pub fn tx_id_hex_to_bytes(tx_id: &str) -> BitcoinTxId {
    let mut bytes = hex::decode(tx_id).expect("ERR_INVALID_TX_ID: not valid hex");
    require!(bytes.len() == 32, "ERR_INVALID_TX_ID: must be 32 bytes");
    bytes.reverse();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    BitcoinTxId(arr)
}

impl Contract {
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
