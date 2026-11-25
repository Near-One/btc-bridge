use near_sdk::serde::{
    self,
    de::{self, Visitor},
    Deserialize, Serialize,
};
use std::{fmt, str::FromStr};

use crate::{env, ext_contract, near, AccountId, Contract, Gas, Promise};
pub mod active_utxo_management;
pub mod deposit;
pub mod withdraw;

pub const GAS_FOR_VERIFY_TRANSACTION_INCLUSION: Gas = Gas::from_tgas(10);
pub const GAS_FOR_GET_LAST_BLOCK_HEIGHT: Gas = Gas::from_tgas(3);

#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct H256(pub [u8; 32]);

#[near(serializers = [borsh, json])]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct U256(u128, u128);

impl FromStr for H256 {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        hex::decode_to_slice(s, &mut result)?;
        result.reverse();
        Ok(H256(result))
    }
}

const TX_VERSION_LEN: usize = 4;
const TX_INPUT_COUNT_LEN: usize = 1;
const COINBASE_PREVOUT_HASH_START: usize = TX_VERSION_LEN + TX_INPUT_COUNT_LEN;
const H256_LEN: usize = 32;

fn parse_coinbase_tx_id(coinbase_tx_id: &str) -> H256 {
    let coinbase_tx_id: H256 = coinbase_tx_id.parse().expect("Invalid coinbase_tx_id");
    crate::require!(
        coinbase_tx_id.0[COINBASE_PREVOUT_HASH_START..H256_LEN]
            .iter()
            .any(|b| *b != 0),
        "coinbase_tx_id matches the start of a coinbase transaction"
    );
    coinbase_tx_id
}

#[near(serializers = [borsh])]
pub struct ProofArgs {
    pub tx_id: H256,
    pub tx_block_blockhash: H256,
    pub tx_index: u64,
    pub merkle_proof: Vec<H256>,
    pub confirmations: u64,
}

impl ProofArgs {
    pub fn new(
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        confirmations: u64,
    ) -> Self {
        ProofArgs {
            tx_id: tx_id.parse().expect("Invalid tx_id"),
            tx_block_blockhash: tx_block_blockhash
                .parse()
                .expect("Invalid tx_block_blockhash"),
            tx_index,
            merkle_proof: merkle_proof
                .into_iter()
                .map(|v| {
                    v.parse()
                        .unwrap_or_else(|_| env::panic_str(&format!("Invalid merkle_proof: {v:?}")))
                })
                .collect(),
            confirmations,
        }
    }
}

#[near(serializers = [borsh])]
pub struct ProofArgsV2 {
    pub tx_id: H256,
    pub tx_block_blockhash: H256,
    pub tx_index: u64,
    pub merkle_proof: Vec<H256>,
    pub coinbase_tx_id: H256,
    pub coinbase_merkle_proof: Vec<H256>,
    pub confirmations: u64,
}

impl ProofArgsV2 {
    pub fn new(
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_tx_id: String,
        coinbase_merkle_proof: Vec<String>,
        confirmations: u64,
    ) -> Self {
        ProofArgsV2 {
            tx_id: tx_id.parse().expect("Invalid tx_id"),
            tx_block_blockhash: tx_block_blockhash
                .parse()
                .expect("Invalid tx_block_blockhash"),
            tx_index,
            merkle_proof: merkle_proof
                .into_iter()
                .map(|v| {
                    v.parse()
                        .unwrap_or_else(|_| env::panic_str(&format!("Invalid merkle_proof: {v:?}")))
                })
                .collect(),
            coinbase_tx_id: parse_coinbase_tx_id(&coinbase_tx_id),
            coinbase_merkle_proof: coinbase_merkle_proof
                .into_iter()
                .map(|v| {
                    v.parse().unwrap_or_else(|_| {
                        env::panic_str(&format!("Invalid coinbase_merkle_proof: {v:?}"))
                    })
                })
                .collect(),
            confirmations,
        }
    }
}

impl<'de> Deserialize<'de> for H256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HexVisitor;

        impl Visitor<'_> for HexVisitor {
            type Value = H256;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a hex string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let mut result = [0; 32];
                hex::decode_to_slice(s, &mut result).map_err(de::Error::custom)?;
                result.reverse();
                Ok(H256(result))
            }
        }

        deserializer.deserialize_str(HexVisitor)
    }
}

impl Serialize for H256 {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
    where
        S: serde::Serializer,
    {
        let reversed: Vec<u8> = self.0.into_iter().rev().collect();
        serializer.serialize_str(&hex::encode(reversed))
    }
}

/// Mirrors `btc-types::contract_args::TxInclusionProof` from the light client contract.
#[near(serializers = [borsh])]
pub struct TxInclusionProof {
    pub tx_id: H256,
    pub tx_block_blockhash: H256,
    pub tx_index: u64,
    pub merkle_proof: Vec<H256>,
    pub coinbase_tx_id: H256,
    pub coinbase_merkle_proof: Vec<H256>,
}

impl TxInclusionProof {
    pub fn new(
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_tx_id: String,
        coinbase_merkle_proof: Vec<String>,
    ) -> Self {
        TxInclusionProof {
            tx_id: tx_id.parse().expect("Invalid tx_id"),
            tx_block_blockhash: tx_block_blockhash
                .parse()
                .expect("Invalid tx_block_blockhash"),
            tx_index,
            merkle_proof: merkle_proof
                .into_iter()
                .map(|v| {
                    v.parse()
                        .unwrap_or_else(|_| env::panic_str(&format!("Invalid merkle_proof: {v:?}")))
                })
                .collect(),
            coinbase_tx_id: parse_coinbase_tx_id(&coinbase_tx_id),
            coinbase_merkle_proof: coinbase_merkle_proof
                .into_iter()
                .map(|v| {
                    v.parse().unwrap_or_else(|_| {
                        env::panic_str(&format!("Invalid coinbase_merkle_proof: {v:?}"))
                    })
                })
                .collect(),
        }
    }
}

/// Mirrors `btc-types::contract_args::TxInclusionInfo` from the light client contract.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug)]
pub struct TxInclusionInfo {
    pub tx_block_height: u64,
    pub mainchain_tip_height: u64,
}

#[ext_contract(ext_btc_light_client)]
pub trait BtcLightClient {
    fn verify_transaction_inclusion(&self, #[serializer(borsh)] args: ProofArgs) -> bool;
    fn verify_transaction_inclusion_with_heights(
        &self,
        #[serializer(borsh)] args: TxInclusionProof,
    ) -> Option<TxInclusionInfo>;
    fn verify_transaction_inclusion_v2(&self, #[serializer(borsh)] args: ProofArgsV2) -> bool;
    fn get_last_block_height(&self) -> u64;
}

impl Contract {
    #[allow(clippy::too_many_arguments)]
    pub fn verify_transaction_inclusion_promise(
        &self,
        btc_light_client_account_id: AccountId,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: Option<(String, Vec<String>)>,
        confirmations: u64,
    ) -> Promise {
        let ext = ext_btc_light_client::ext(btc_light_client_account_id)
            .with_static_gas(GAS_FOR_VERIFY_TRANSACTION_INCLUSION);
        if let Some((coinbase_tx_id, coinbase_merkle_proof)) = coinbase_proof {
            ext.verify_transaction_inclusion_v2(ProofArgsV2::new(
                tx_id,
                tx_block_blockhash,
                tx_index,
                merkle_proof,
                coinbase_tx_id,
                coinbase_merkle_proof,
                confirmations,
            ))
        } else {
            ext.verify_transaction_inclusion(ProofArgs::new(
                tx_id,
                tx_block_blockhash,
                tx_index,
                merkle_proof,
                confirmations,
            ))
        }
    }

    /// The LC only proves inclusion here; the confirmations check happens in
    /// the callback on this side.
    pub fn verify_transaction_inclusion_with_heights_promise(
        &self,
        btc_light_client_account_id: AccountId,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        coinbase_proof: (String, Vec<String>),
    ) -> Promise {
        let (coinbase_tx_id, coinbase_merkle_proof) = coinbase_proof;
        ext_btc_light_client::ext(btc_light_client_account_id)
            .with_static_gas(GAS_FOR_VERIFY_TRANSACTION_INCLUSION)
            .verify_transaction_inclusion_with_heights(TxInclusionProof::new(
                tx_id,
                tx_block_blockhash,
                tx_index,
                merkle_proof,
                coinbase_tx_id,
                coinbase_merkle_proof,
            ))
    }

    pub fn get_last_block_height_promise(&self) -> Promise {
        let config = self.internal_config();
        ext_btc_light_client::ext(config.btc_light_client_account_id.clone())
            .with_static_gas(GAS_FOR_GET_LAST_BLOCK_HEIGHT)
            .get_last_block_height()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_coinbase_tx_id;
    use bitcoin::hashes::Hash;

    #[test]
    fn parse_coinbase_tx_id_accepts_coinbase_txid() {
        let genesis = bitcoin::blockdata::constants::genesis_block(bitcoin::Network::Bitcoin);
        let coinbase_txid = genesis.txdata[0].compute_txid();

        let coinbase_tx_id = parse_coinbase_tx_id(&coinbase_txid.to_string());

        assert_eq!(
            coinbase_tx_id.0,
            coinbase_txid.to_raw_hash().to_byte_array()
        );
    }

    #[test]
    #[should_panic(expected = "coinbase_tx_id matches the start of a coinbase transaction")]
    fn parse_coinbase_tx_id_rejects_coinbase_tx_prefix() {
        let mut prefix = [0u8; 32];
        prefix[..4].copy_from_slice(&2u32.to_le_bytes());
        prefix[4] = 1;
        prefix.reverse();

        parse_coinbase_tx_id(&hex::encode(prefix));
    }
}
