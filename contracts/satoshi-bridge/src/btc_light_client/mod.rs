use near_sdk::serde::{
    self,
    de::{self, Visitor},
    Deserialize, Serialize,
};
use std::{fmt, str::FromStr};

#[cfg(not(feature = "dash"))]
use crate::require;
use crate::{env, ext_contract, near, AccountId, Contract, Gas, Promise};
pub mod active_utxo_management;
pub mod deposit;
pub mod withdraw;

/// Maximum expected byte length for the verification promise result.
/// For light client (BTC/non-dash): bool result (~10 bytes).
/// For MPC (DASH): VerifyForeignTransactionResponse (~1024 bytes).
#[cfg(not(feature = "dash"))]
pub(crate) const MAX_VERIFY_RESULT: usize = crate::MAX_BOOL_RESULT;
#[cfg(feature = "dash")]
pub(crate) const MAX_VERIFY_RESULT: usize = crate::MAX_MPC_VERIFY_RESULT;

/// Assert that the transaction verification promise (light client or MPC) succeeded.
///
/// For non-DASH (light client): parses the `bool` return value and requires it to be `true`.
/// For DASH (MPC): a successful promise means the MPC network confirmed the transaction.
/// The MPC contract's `verify_foreign_transaction` panics if verification fails,
/// so reaching the callback means the transaction is verified.
pub(crate) fn assert_verification_succeeded() {
    let result_bytes =
        env::promise_result_checked(0, MAX_VERIFY_RESULT).expect("Transaction verification failed");

    #[cfg(not(feature = "dash"))]
    {
        let is_valid = crate::serde_json::from_slice::<bool>(&result_bytes)
            .expect("verify_transaction_inclusion returned invalid result");
        require!(is_valid, "verify_transaction_inclusion returned false");
    }

    #[cfg(feature = "dash")]
    {
        let _ = result_bytes;
    }
}

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
                        .unwrap_or_else(|_| env::panic_str("Invalid merkle_proof: {v:?}"))
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

#[ext_contract(ext_btc_light_client)]
pub trait BtcLightClient {
    fn verify_transaction_inclusion(&self, #[serializer(borsh)] args: ProofArgs) -> bool;
    fn get_last_block_height(&self) -> u32;
}

impl Contract {
    pub fn verify_transaction_inclusion_promise(
        &self,
        btc_light_client_account_id: AccountId,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        confirmations: u64,
    ) -> Promise {
        ext_btc_light_client::ext(btc_light_client_account_id)
            .with_static_gas(GAS_FOR_VERIFY_TRANSACTION_INCLUSION)
            .verify_transaction_inclusion(ProofArgs::new(
                tx_id.clone(),
                tx_block_blockhash,
                tx_index,
                merkle_proof,
                confirmations,
            ))
    }

    pub fn get_last_block_height_promise(&self) -> Promise {
        let config = self.internal_config();
        ext_btc_light_client::ext(config.btc_light_client_account_id.clone())
            .with_static_gas(GAS_FOR_GET_LAST_BLOCK_HEIGHT)
            .get_last_block_height()
    }
}
