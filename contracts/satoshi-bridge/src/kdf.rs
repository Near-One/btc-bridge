use crate::*;

use crate::network::Address;
use k256::elliptic_curve::sec1::ToEncodedPoint;

impl Contract {
    pub fn generate_public_key(&self, path: &str) -> Vec<u8> {
        let mpc_pk = crypto_shared::near_public_key_to_affine_point(
            self.internal_config()
                .chain_signatures_root_public_key
                .clone()
                .expect("Missing chain_signatures_root_public_key"),
        );
        let epsilon = crypto_shared::derive_epsilon(&env::current_account_id(), path);
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
