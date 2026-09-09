use near_sdk::{env, near};

use crate::DepositTxSummary;

#[near(serializers = [json])]
pub struct ChainSpecificData {}

/// Bitcoin stand-in for the Zcash ZIP-244 compact deposit proof, so that the
/// shared `DepositTxProof` compiles on both chains.
///
/// There is no Bitcoin equivalent: a Bitcoin txid is a flat `SHA256d` over the
/// whole serialization, not a hash tree, so the inputs cannot be replaced by a
/// digest. Bitcoin deposits must pass the full transaction bytes.
#[near(serializers = [json])]
pub struct CompactTxProof {}

impl CompactTxProof {
    pub fn resolve(self) -> DepositTxSummary {
        env::panic_str("A compact tx_bytes proof is not supported on Bitcoin")
    }
}
