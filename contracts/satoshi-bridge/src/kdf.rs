use crate::*;

use std::str::FromStr;

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

    pub fn generate_btc_p2wpkh_address(&self, path: &str) -> Address {
        let btc_public_key = self.generate_btc_public_key(path);
        Address::p2wpkh(&btc_public_key.try_into().unwrap(), btc_network())
    }
}

pub fn string_to_btc_address(address_string: &str) -> Address {
    Address::from_str(address_string)
        .unwrap_or_else(|_| panic!("{address_string} not a valid btc address"))
        .require_network(btc_network())
        .unwrap_or_else(|_| panic!("{address_string} network error"))
}

pub fn btc_network() -> Network {
    if env::current_account_id().to_string().ends_with(".near") {
        Network::Bitcoin
    } else {
        Network::Testnet
    }
}
