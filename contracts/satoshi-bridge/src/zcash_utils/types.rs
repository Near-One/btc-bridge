use near_sdk::json_types::Base64VecU8;
use near_sdk::near;

use crate::TxInclusionProof;

#[near(serializers = [json])]
pub struct ChainSpecificData {
    pub orchard_bundle_bytes: Base64VecU8,
    pub expiry_height: u32,
}

impl TxInclusionProof {
    /// Returns the coinbase txid for the light client.
    /// On Zcash `coinbase_tx_id` is the coinbase txid itself.
    pub fn coinbase_tx_id(&self) -> String {
        self.coinbase_tx_id.clone()
    }
}
