use near_sdk::{env, near};

use crate::{DepositTxSummary, TxInclusionProof};

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

impl TxInclusionProof {
    /// Returns the coinbase txid for the light client.
    /// On Bitcoin `coinbase_tx_id` carries the single SHA256 of the coinbase's
    /// non-witness serialization, so the txid is its SHA256, computed here.
    pub fn coinbase_tx_id(&self) -> String {
        let mut coinbase_tx_sha256 = [0u8; 32];
        hex::decode_to_slice(&self.coinbase_tx_id, &mut coinbase_tx_sha256)
            .unwrap_or_else(|_| env::panic_str("Invalid coinbase_tx_id"));
        let mut coinbase_tx_id = env::sha256_array(coinbase_tx_sha256);
        coinbase_tx_id.reverse();
        hex::encode(coinbase_tx_id)
    }
}

#[cfg(test)]
mod tests {
    use crate::TxInclusionProof;
    use bitcoin::hashes::{sha256, Hash};

    #[test]
    fn coinbase_tx_id_derives_coinbase_txid() {
        let genesis = bitcoin::blockdata::constants::genesis_block(bitcoin::Network::Bitcoin);
        let coinbase = &genesis.txdata[0];
        let coinbase_tx_sha256 = hex::encode(
            sha256::Hash::hash(&bitcoin::consensus::serialize(coinbase)).to_byte_array(),
        );
        let proof = TxInclusionProof {
            tx_block_blockhash: genesis.block_hash().to_string(),
            tx_index: 0,
            merkle_proof: vec![],
            coinbase_tx_id: coinbase_tx_sha256,
            coinbase_merkle_proof: vec![],
        };

        let coinbase_tx_id = proof.coinbase_tx_id();

        assert_eq!(coinbase_tx_id, coinbase.compute_txid().to_string());
        assert_eq!(
            coinbase_tx_id,
            "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid coinbase_tx_id")]
    fn coinbase_tx_id_rejects_invalid_coinbase_tx_id() {
        let proof = TxInclusionProof {
            tx_block_blockhash: String::new(),
            tx_index: 0,
            merkle_proof: vec![],
            coinbase_tx_id: "00".to_string(),
            coinbase_merkle_proof: vec![],
        };
        proof.coinbase_tx_id();
    }
}
