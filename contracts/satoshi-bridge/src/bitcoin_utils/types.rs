use near_sdk::{env, near};

use crate::DepositTxSummary;

#[near(serializers = [json])]
pub struct ChainSpecificData {}

/// Stand-in so that the shared `DepositTxProof` compiles on Bitcoin, where a
/// txid is a flat `SHA256d` rather than a hash tree and has no compact form.
#[near(serializers = [json])]
pub struct CompactTxProof {}

impl CompactTxProof {
    pub fn resolve(self) -> DepositTxSummary {
        env::panic_str("A compact tx_bytes proof is not supported on Bitcoin")
    }
}
