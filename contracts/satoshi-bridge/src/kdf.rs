use crate::{env, network::Address, BtcPublicKey, Contract};

use k256::elliptic_curve::{bigint::ArrayEncoding, sec1::ToEncodedPoint, PrimeField};
use sha3::{Digest, Sha3_256};

const EPSILON_DERIVATION_PREFIX: &str = "near-mpc-recovery v0.1.0 epsilon derivation:";

fn derive_epsilon(account_id: &str, path: &str) -> k256::Scalar {
    let derivation_path = format!("{EPSILON_DERIVATION_PREFIX}{},{}", account_id, path);
    let mut hasher = Sha3_256::new();
    hasher.update(derivation_path);
    let hash: [u8; 32] = hasher.finalize().into();
    let bytes = k256::U256::from_be_slice(hash.as_slice());
    k256::Scalar::from_repr(bytes.to_be_byte_array())
        .expect("Derived epsilon value falls outside of the field")
}

impl Contract {
    pub fn get_public_key_by_path(&self, path: String) -> String {
        let public_key_bytes = self.generate_public_key(&path);
        let uncompressed_btc_public_key =
            BtcPublicKey::from_slice(&public_key_bytes).expect("Invalid public key bytes");

        uncompressed_btc_public_key.inner.to_string()
    }
}

impl Contract {
    pub fn generate_public_key(&self, path: &str) -> Vec<u8> {
        let mpc_pk = crypto_shared::near_public_key_to_affine_point(
            self.internal_config()
                .chain_signatures_root_public_key
                .clone()
                .expect("Missing chain_signatures_root_public_key"),
        );
        let epsilon = derive_epsilon(env::current_account_id().as_ref(), path);
        let user_pk = crypto_shared::derive_key(mpc_pk, epsilon);
        let user_pk_encoded_point = user_pk.to_encoded_point(false);
        user_pk_encoded_point.as_bytes().to_vec()
    }

    pub fn generate_btc_public_key(&self, path: &str) -> BtcPublicKey {
        let public_key_bytes = self.generate_public_key(path);
        let uncompressed_btc_public_key =
            BtcPublicKey::from_slice(&public_key_bytes).expect("Invalid public key bytes");
        uncompressed_btc_public_key
            .inner
            .to_string()
            .parse()
            .unwrap()
    }

    pub fn generate_utxo_chain_address(&self, path: &str) -> Address {
        let btc_public_key = self.generate_btc_public_key(path);
        Address::from_pubkey(self.internal_config().chain.clone(), btc_public_key)
            .expect("Invalid public key")
    }
}

#[cfg(test)]
mod tests {
    use super::derive_epsilon;

    #[test]
    fn test_derive_epsilon_matches_crypto_shared() {
        let account_id = "test.near";
        let path = "bitcoin-1";
        let local = derive_epsilon(account_id, path);
        let v1_id: near_account_id_v1::AccountId = account_id.parse().unwrap();
        let original = crypto_shared::derive_epsilon(&v1_id, path);
        assert_eq!(local.to_bytes(), original.to_bytes());
    }
}
