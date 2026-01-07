use bitcoin::hashes::Hash;

use crate::{env, Timestamp};

pub const UTXO_STORAGE_KEY_TAG: &str = "@";

pub fn generate_utxo_storage_key(txid: String, vout: u32) -> String {
    format!(
        "{}{}{}",
        txid,
        UTXO_STORAGE_KEY_TAG,
        vout.to_string().as_str()
    )
}

pub fn to_nano(sec: u32) -> Timestamp {
    Timestamp::from(sec) * 10u64.pow(9)
}

pub fn nano_to_sec(nano: u64) -> u32 {
    u32::try_from(nano / 10u64.pow(9))
        .unwrap_or_else(|_| env::panic_str("Timestamp overflow when converting nano to sec"))
}

pub mod u64_dec_format {
    use near_sdk::serde::de;
    use near_sdk::serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(num: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&num.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

pub mod u128_dec_format {
    use near_sdk::serde::de;
    use near_sdk::serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(num: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&num.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

pub fn verify_secp256k1_signature(public_key: &Vec<u8>, message: &str, signature: &[u8]) -> bool {
    let mut recovery_id = signature[0];
    if recovery_id >= 31 {
        recovery_id -= 31;
    } else {
        recovery_id -= 27;
    }
    let msg_hash = bitcoin::sign_message::signed_msg_hash(message).to_byte_array();
    let recovered_pk = env::ecrecover(&msg_hash, &signature[1..65], recovery_id, true)
        .unwrap()
        .to_vec();
    &recovered_pk == public_key
}
