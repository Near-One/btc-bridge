use std::borrow::{Borrow, BorrowMut};

use crate::psbt_wrapper::PsbtWrapper;
use crate::*;

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct OriginalState {
    pub stage: PendingInfoStage,
    #[serde(with = "u128_dec_format")]
    pub max_gas_fee: u128,
    pub last_rbf_time_sec: Option<u32>,
    pub cancel_rbf_reserved: Option<U128>,
}

impl OriginalState {
    pub fn assert_pending_sign(&self) {
        require!(
            self.stage == PendingInfoStage::PendingSign,
            "Not pending sign stage"
        );
    }
    pub fn assert_pending_verify(&self) {
        require!(
            self.stage == PendingInfoStage::PendingVerify,
            "Not pending verify stage"
        );
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct RbfState {
    pub stage: PendingInfoStage,
    pub original_tx_id: String,
}

impl RbfState {
    pub fn assert_pending_sign(&self) {
        require!(
            self.stage == PendingInfoStage::PendingSign,
            "Not pending sign stage"
        );
    }
    pub fn assert_pending_verify(&self) {
        require!(
            self.stage == PendingInfoStage::PendingVerify,
            "Not pending verify stage"
        );
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub enum PendingInfoStage {
    PendingSign,
    PendingVerify,
    PendingBurn,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub enum PendingInfoState {
    WithdrawOriginal(OriginalState),
    WithdrawUserRbf(RbfState),
    WithdrawCancelRbf(RbfState),
    ActiveUtxoManagementOriginal(OriginalState),
    ActiveUtxoManagementRbf(RbfState),
    ActiveUtxoManagementCancelRbf(RbfState),
}

impl PendingInfoState {
    pub fn assert_pending_sign(&self) {
        match self {
            PendingInfoState::WithdrawOriginal(state) => state.assert_pending_sign(),
            PendingInfoState::WithdrawUserRbf(state) => state.assert_pending_sign(),
            PendingInfoState::WithdrawCancelRbf(state) => state.assert_pending_sign(),
            PendingInfoState::ActiveUtxoManagementOriginal(state) => state.assert_pending_sign(),
            PendingInfoState::ActiveUtxoManagementRbf(state) => state.assert_pending_sign(),
            PendingInfoState::ActiveUtxoManagementCancelRbf(state) => state.assert_pending_sign(),
        }
    }
    pub fn assert_pending_verify(&self) {
        match self {
            PendingInfoState::WithdrawOriginal(state) => state.assert_pending_verify(),
            PendingInfoState::WithdrawUserRbf(state) => state.assert_pending_verify(),
            PendingInfoState::WithdrawCancelRbf(state) => state.assert_pending_verify(),
            PendingInfoState::ActiveUtxoManagementOriginal(state) => state.assert_pending_verify(),
            PendingInfoState::ActiveUtxoManagementRbf(state) => state.assert_pending_verify(),
            PendingInfoState::ActiveUtxoManagementCancelRbf(state) => state.assert_pending_verify(),
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct BTCPendingInfo {
    pub account_id: AccountId,
    pub btc_pending_id: String,
    #[serde(with = "u128_dec_format")]
    pub transfer_amount: u128,
    #[serde(with = "u128_dec_format")]
    pub actual_received_amount: u128,
    #[serde(with = "u128_dec_format")]
    pub withdraw_fee: u128,
    #[serde(with = "u128_dec_format")]
    pub gas_fee: u128,
    #[serde(with = "u128_dec_format")]
    pub burn_amount: u128,
    pub psbt_hex: String,
    pub vutxos: Vec<VUTXO>,
    pub signatures: Vec<Option<SignatureResponse>>,
    pub tx_bytes_with_sign: Option<Vec<u8>>,
    pub create_time_sec: u32,
    pub last_sign_time_sec: u32,
    pub state: PendingInfoState,
    #[cfg(feature = "zcash")]
    pub expiry_height: u32,
}

impl BTCPendingInfo {
    pub fn assert_pending_sign(&self) {
        self.state.assert_pending_sign();
    }

    pub fn assert_pending_verify(&self) {
        self.state.assert_pending_verify();
    }

    pub fn is_cancel_withdraw_rbf(&self) -> bool {
        matches!(self.state, PendingInfoState::WithdrawCancelRbf(..))
    }

    pub fn get_original_tx_id(&self) -> Option<&String> {
        match self.state.borrow() {
            PendingInfoState::WithdrawUserRbf(state) => Some(state.original_tx_id.borrow()),
            PendingInfoState::WithdrawCancelRbf(state) => Some(state.original_tx_id.borrow()),
            PendingInfoState::ActiveUtxoManagementRbf(state) => Some(state.original_tx_id.borrow()),
            PendingInfoState::ActiveUtxoManagementCancelRbf(state) => {
                Some(state.original_tx_id.borrow())
            }
            _ => None,
        }
    }

    pub fn assert_withdraw_related_pending_verify_tx(&self) {
        match self.state.borrow() {
            PendingInfoState::WithdrawOriginal(state) => state.assert_pending_verify(),
            PendingInfoState::WithdrawUserRbf(state) => state.assert_pending_verify(),
            PendingInfoState::WithdrawCancelRbf(state) => state.assert_pending_verify(),
            _ => env::panic_str("Not withdraw related tx"),
        }
    }

    pub fn assert_active_utxo_management_related_pending_verify_tx(&self) {
        match self.state.borrow() {
            PendingInfoState::ActiveUtxoManagementOriginal(state) => state.assert_pending_verify(),
            PendingInfoState::ActiveUtxoManagementRbf(state) => state.assert_pending_verify(),
            PendingInfoState::ActiveUtxoManagementCancelRbf(state) => state.assert_pending_verify(),
            _ => env::panic_str("Not active utxo management related tx"),
        };
    }

    pub fn assert_active_utxo_management_original_pending_verify_tx(&self) {
        match self.state.borrow() {
            PendingInfoState::ActiveUtxoManagementOriginal(state) => state.assert_pending_verify(),
            _ => env::panic_str("Not active utxo management original tx"),
        };
    }

    pub fn assert_withdraw_original_pending_verify_tx(&self) {
        match self.state.borrow() {
            PendingInfoState::WithdrawOriginal(state) => state.assert_pending_verify(),
            _ => env::panic_str("Not withdraw original tx"),
        };
    }

    pub fn get_max_gas_fee(&self) -> u128 {
        match self.state.borrow() {
            PendingInfoState::WithdrawOriginal(state) => state.max_gas_fee,
            PendingInfoState::ActiveUtxoManagementOriginal(state) => state.max_gas_fee,
            _ => env::panic_str("Not original tx"),
        }
    }

    pub fn update_max_gas_fee(&mut self, gas_fee: u128) {
        match self.state.borrow_mut() {
            PendingInfoState::WithdrawOriginal(state) => {
                state.max_gas_fee = gas_fee;
                state.last_rbf_time_sec = Some(nano_to_sec(env::block_timestamp()));
            }
            PendingInfoState::ActiveUtxoManagementOriginal(state) => {
                state.max_gas_fee = gas_fee;
                state.last_rbf_time_sec = Some(nano_to_sec(env::block_timestamp()));
            }
            _ => env::panic_str("Not original tx"),
        }
    }

    pub fn to_pending_verify_stage(&mut self) {
        match self.state.borrow_mut() {
            PendingInfoState::WithdrawOriginal(state) => {
                state.stage = PendingInfoStage::PendingVerify
            }
            PendingInfoState::WithdrawUserRbf(state) => {
                state.stage = PendingInfoStage::PendingVerify
            }
            PendingInfoState::WithdrawCancelRbf(state) => {
                state.stage = PendingInfoStage::PendingVerify
            }
            PendingInfoState::ActiveUtxoManagementOriginal(state) => {
                state.stage = PendingInfoStage::PendingVerify
            }
            PendingInfoState::ActiveUtxoManagementRbf(state) => {
                state.stage = PendingInfoStage::PendingVerify
            }
            PendingInfoState::ActiveUtxoManagementCancelRbf(state) => {
                state.stage = PendingInfoStage::PendingVerify
            }
        }
    }

    pub fn to_pending_burn_stage(&mut self) {
        match self.state.borrow_mut() {
            PendingInfoState::WithdrawOriginal(state) => {
                state.stage = PendingInfoStage::PendingBurn
            }
            PendingInfoState::WithdrawUserRbf(state) => state.stage = PendingInfoStage::PendingBurn,
            PendingInfoState::WithdrawCancelRbf(state) => {
                state.stage = PendingInfoStage::PendingBurn
            }
            PendingInfoState::ActiveUtxoManagementOriginal(state) => {
                state.stage = PendingInfoStage::PendingBurn
            }
            PendingInfoState::ActiveUtxoManagementRbf(state) => {
                state.stage = PendingInfoStage::PendingBurn
            }
            PendingInfoState::ActiveUtxoManagementCancelRbf(state) => {
                state.stage = PendingInfoStage::PendingBurn
            }
        }
    }

    pub fn do_cancel(&mut self, gas_fee: u128, cancel_rbf_reserved: u128) {
        match self.state.borrow_mut() {
            PendingInfoState::WithdrawOriginal(state) => {
                state.max_gas_fee = gas_fee;
                state.last_rbf_time_sec = Some(nano_to_sec(env::block_timestamp()));
                state.cancel_rbf_reserved = Some(cancel_rbf_reserved.into());
            }
            PendingInfoState::ActiveUtxoManagementOriginal(state) => {
                state.max_gas_fee = gas_fee;
                state.last_rbf_time_sec = Some(nano_to_sec(env::block_timestamp()));
                state.cancel_rbf_reserved = Some(cancel_rbf_reserved.into());
            }
            _ => env::panic_str("Not original tx"),
        }
    }

    pub fn get_cancel_rbf_reserved(&self) -> Option<U128> {
        match &self.state {
            PendingInfoState::WithdrawOriginal(state) => state.cancel_rbf_reserved,
            PendingInfoState::ActiveUtxoManagementOriginal(state) => state.cancel_rbf_reserved,
            _ => env::panic_str("Not original tx"),
        }
    }

    pub fn assert_not_canceled(&self) {
        require!(self.get_cancel_rbf_reserved().is_none(), "already canceled");
    }

    pub fn is_all_signed(&self) -> bool {
        self.signatures.iter().all(|v| v.is_some())
    }

    pub fn get_psbt(&self) -> PsbtWrapper {
        PsbtWrapper::deserialize(&self.psbt_hex)
    }

    pub fn get_transaction(&self) -> crate::transaction::Transaction {
        bytes_to_btc_transaction(
            self.tx_bytes_with_sign
                .as_ref()
                .expect("Missing tx_bytes_with_sign"),
        )
    }
}

#[near(serializers = [borsh])]
pub enum VBTCPendingInfo {
    Current(BTCPendingInfo),
}

impl From<VBTCPendingInfo> for BTCPendingInfo {
    fn from(v: VBTCPendingInfo) -> Self {
        match v {
            VBTCPendingInfo::Current(c) => c,
        }
    }
}

impl From<&VBTCPendingInfo> for BTCPendingInfo {
    fn from(v: &VBTCPendingInfo) -> Self {
        match v {
            VBTCPendingInfo::Current(c) => c.clone(),
        }
    }
}

impl<'a> From<&'a VBTCPendingInfo> for &'a BTCPendingInfo {
    fn from(v: &'a VBTCPendingInfo) -> Self {
        match v {
            VBTCPendingInfo::Current(c) => c,
        }
    }
}

impl<'a> From<&'a mut VBTCPendingInfo> for &'a mut BTCPendingInfo {
    fn from(v: &'a mut VBTCPendingInfo) -> Self {
        match v {
            VBTCPendingInfo::Current(c) => c,
        }
    }
}

impl From<BTCPendingInfo> for VBTCPendingInfo {
    fn from(c: BTCPendingInfo) -> Self {
        VBTCPendingInfo::Current(c)
    }
}

impl Contract {
    pub fn check_btc_pending_info_exists(&self, btc_pending_id: &String) -> bool {
        self.data().btc_pending_infos.contains_key(btc_pending_id)
    }

    pub fn internal_view_btc_pending_info(
        &self,
        btc_pending_id: &String,
    ) -> Option<BTCPendingInfo> {
        self.data()
            .btc_pending_infos
            .get(btc_pending_id)
            .map(|o| o.into())
    }

    pub fn internal_unwrap_btc_pending_info(&self, btc_pending_id: &String) -> &BTCPendingInfo {
        self.data()
            .btc_pending_infos
            .get(btc_pending_id)
            .map(|o| o.into())
            .expect("BTC pending info not exist")
    }

    pub fn internal_unwrap_mut_btc_pending_info(
        &mut self,
        btc_pending_id: &String,
    ) -> &mut BTCPendingInfo {
        self.data_mut()
            .btc_pending_infos
            .get_mut(btc_pending_id)
            .map(|o| o.into())
            .expect("BTC pending info not exist")
    }

    pub fn internal_remove_btc_pending_info(&mut self, btc_pending_id: &String) -> BTCPendingInfo {
        self.data_mut()
            .btc_pending_infos
            .remove(btc_pending_id)
            .expect("BTC pending info not exist")
            .into()
    }

    pub fn internal_clear_invalid_pending_verify_rbf(&mut self, btc_pending_id: String) {
        let btc_pending_info = self.internal_remove_btc_pending_info(&btc_pending_id);
        btc_pending_info.assert_pending_verify();
        let original_tx_id = btc_pending_info
            .get_original_tx_id()
            .expect("Not rbf transaction");
        require!(
            !self.data().rbf_txs.contains_key(original_tx_id),
            "Not invalid pending verify rbf"
        );
    }
}

pub fn generate_btc_pending_sign_id(payload_preimages: &[Vec<u8>]) -> String {
    let hash_bytes = env::sha256_array(
        &payload_preimages
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<u8>>(),
    );
    hex::encode(hash_bytes)
}

pub fn bytes_to_btc_transaction(tx_bytes: &[u8]) -> crate::transaction::Transaction {
    crate::transaction::Transaction::decode(tx_bytes).expect("Deserialization tx_bytes failed")
}

#[cfg(test)]
mod tests {
    use crate::network::{Address, Chain};
    use crate::{bytes_to_btc_transaction, get_deposit_path, DepositMsg};
    use bitcoin::PublicKey as BtcPublicKey;
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    use near_sdk::{env, PublicKey};
    use std::str::FromStr;
    pub fn generate_public_key(path: &str) -> Vec<u8> {
        let mpc_pk = crypto_shared::near_public_key_to_affine_point(
            PublicKey::from_str("secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3").unwrap(),
        );
        let epsilon = crypto_shared::derive_epsilon(
            &"zcash_connector-20250714-143829.testnet".parse().unwrap(),
            path,
        );
        let user_pk = crypto_shared::derive_key(mpc_pk, epsilon);
        let user_pk_encoded_point = user_pk.to_encoded_point(false);
        user_pk_encoded_point.as_bytes().to_vec()
    }

    pub fn generate_btc_public_key(path: &str) -> BtcPublicKey {
        let public_key_bytes = generate_public_key(path);
        let uncompressed_btc_public_key =
            BtcPublicKey::from_slice(&public_key_bytes).expect("Invalid public key bytes");
        uncompressed_btc_public_key
            .inner
            .to_string()
            .parse()
            .unwrap()
    }

    pub fn generate_utxo_chain_address(path: &str) -> Address {
        let btc_public_key = generate_btc_public_key(path);
        Address::from_pubkey(Chain::ZcashTestnet, btc_public_key)
    }

    #[test]
    fn test_zcash_tx_bytes() {
        let tx_zcash_hex = "050000800a27a7265510e7c80000000085443500000160ae0a00000000001976a914f97c1d1cb17b5657d889cf7503a69e53c9da081988ac000002af1f4b1e9f842c3f878efee2b9ab4403500c5fc1ab3eb14442ad3463880cc387c8769e71b1a858a7aa268cfc17b6c4e414e69e2cc257e2f289d4780516d4a82149fdc0abd4061824c43a7c17f94a7bc0736eeaa20429dffca2180c8094ab779acbc03ab3bdf2c30bd31a859c96032ffda141549d189045ec43ac1b9dc6f15c2875afe1f1fa3ff2bb4b03599506062fe118a097008de7d5af99f09a78819bb93fe143e8c888eeab1ed2421660ca46d416bf6b9da01e54663bf18ad79dfe6bcd46587b32fb47efb0b49e1a99f6ec6812fdaa9d178eb21f6487adcdac88553580c61137b7b7169029cb26bb8c09869ba866ff8185d4d31e2d6c431ed86338fea3db03faf5b164d851d00e4dbaf608b8415a31196d3fd33246490252c8f6bb7bc3dfe0d9400bab0ce0015a9e23cf730e5aa5211d9906aef91c4591c4d2b2e3cc913809990f1bd0fdc3b828ea5fd416e1459a12364b502f65d9eda45615850942991ed3f7aa4b12700c6493bc89e969b2320d11d0b5deea814c2d9f2c20fd2635f7eb1172a9009ed31fa3e281a7632b68a023fbf232e890023505fa08f806c3f71b31f76ac480795e01d8249758c30b18f9be9d126b077a338bc1aa8e2a4d1ebea6f48717cad84488be5b7f7a1a323f4016c0d38c2bba099420ad2bab8a68068831264de585c8f2d62dbe8bab7b692a6047793774d40b9b3311a824b670ea64fa80389a1923f49e5485c850665c53d160c2bb4c230576099057f9167270678a82e80ab4e05c28e1ecd57b8145ad3b11bbca8c160478f44086c5f2d43417e78d2aa19ec136f9d6e34306da957990a125e45df52dccf8efce76df8b5ec2fe3cc17b855c00e18ef62f84de0b1d1085973967222f1e7c8230943cf7a51c4c50ce343117b278c81af6241ddca5af47e2afef683fc66aa194d90fcefb13cb58a478e9765e323d8a0da292c5792455dc9434e53292b8b8cec2da1c1b2c39f00aff5081aa21adfbdea6f7fd153794bc94701a7aae3fa5bc94060452ca1c1d965fe4e37f60315f87d091c79caa9dfe33261c377d837ef273436615d295be073385bc4374bf743aeb4df84622407afee6e63a8c9389ece0e50c8ebf031b0a61dd9615c5ef92d99e3b01345afd1cfa4216f10da8ccfc2d56314c5c0f49c69f33a19c073e655f606b55ea19b91bdcc9a7d4c054e9e3bbd5efac4ddd213b666ea6f1a0612e765a38a94fc1edd5f6c6dd5dba50db01a1172436ef86e30fb8ffd055670f9fa83f28d5a757da33feb35ff4ada98bbee715f2b9883f109420943462e51fee9ad4282ee6876820c74d32b30a1bcaa5b22ddc73dc62e28ee516afb866a0305dc7e8b3ca3cf380187b33076932fbbdea8339f59c6360adefa086d85019fdcf441cb724a0f890775e6fd4e00ed950bf618e5f258e1c485f9c3b512e616126eba07b3d4ae08046a446a52c8107a5a62ef8ecb4662dfaf53b05e8e71bcc537a3e32e225920a4b989432ad47b5c31897919bc989c6f48cac1579c1f525b481c0bcc9e019bae4d9eeeef18e4e33888f1cc00df3c06408ab6b4064b40496dce7c905b8c19335025debe5f54ad535d7395e3b4f75989983ed76791230ab52bfe98e562175d4f0cb1933a0929aed49a7a0e1e76638dab0e3703f5577ba6e3088f44e590633cf13d9b679ca7f7561e9906bddd6a05244be421003b3a1a54693cbef7ce2632040b4be9072f7a632fa3a9afbfdb25414bda9ee82866c439d0fbb148e294e9bcc8759aae703d37f9d492c3afddf5b1f6fe93a8b059e7d6c5ab53667262656746e405515fe4a7ddc156b8e8f3c6a9e345fdf49e6ff654b6111022198324655a522b58c6ef9a14b78b739802ec8b87b5cb1ad986aa769ba2958ee98bcc00f7e86e806f2d344e22ec25d57abefa84d3de3440c5fb779784c5b0d240fe9cea509d76744a8a06ff0f9caed6095f0f56c89451cfef6eec39470016c9b99cd7736821b9b426d4b3731faca55e903756e9506712e5c0cc63ea040e6ba5396797e42edecef69dedebb725acb6678fc9ccba24873659a8b7a253dfece816d5f1028197ca09574488c4405b2b3129d2dde643ba6852878eb337baf9f7a2388e6c29a9ee281f3fe1c96647c5bcb485e60953d895e4016cf2a679425450de1c7ffe99cc4dcc141dde4c480b445544bda0afb18c0465c75de100b24547ec90c978939ae32a6d14b5a61380382aa75652963aa9d140a66ff0016bb50fcd11749a24601224ae4f444ffb5d80a618f7843a6b5a88a74bf2149a8a3e08be36194e76902b98a1134123fae6474056b860b474762125d1803f8e80a000000000032861203d8fd375ba5aa472daea41c5c9d2a015551d67437e267bbec1fe2e109fd601c6e99d1bdf653d9b2595d9f9898341989654e99c643e98bc7cd196dd5d946630009a3c02403849254ca01f6afcff0c28a622fd397980e2fa8f35b49ace0ece09b75499feaf26293d18f917ecde6bc9f930614661ce1d32d76d0e863e520fb4583174b43c77c3e6859b53f02447159dcb02069d7b5eabf504c96829c5cc50d742043ddacf37eb14759b345f3851d4aa397c0b4fe99bf0ff748e2a538ace4275e22a063da15d86ff72572c480c4fe1760318b141c440732d72947da4e27578eef24762543315eb0e4d8c487ca449ec2a1e60b5205722f0d16e5583831bb7fbf9fa40a0cd5a6a9ec36d8f68d779c36fbd09122f59f4f9f4179c5d45a2301a5fcf292600e3e930c4c9819a659bce76799df2c7dd78163a59fe4aea354aeed56a2ea15d18c9bb5001c1774930eca2ed94f1010785e3c92bfb2633c80ea42a3373d301657e21fb540d6e6dd869c79f6b846ee225762ebac40d0949e56d4f85f09e4ad981e7af031a0b5e8fac04f0a6fe02acac9a499903fcdad4ec8c997f1cd88cd2120a53f2726ed09b8d05ded97b637401bb74f5a1cf408c60a112b7f4bcfe18685baa22eda0e16f02f423c8678e7cbbcc99f132c1ba2c0d6c05ba83d43d7b5b5ab0cb00c1685fff14e5e8592edae5a276186ca43481e55a340761ff4a29f0b7def0a6f23ea96fdecbbfa53bc3270d0acb38fa352143ba2632565d623070c9b72ed9e7e0d43e547d7864fcf507b110c3a36f4a6745e916d945bc3803f6847ca774a8fd9d2104d9436efad03ac5daac19cd7cb1f70c542e971b817d5ef3566e2ccac26c48f7d49dc2176ba1c24263c61f70411e76ab1e72714c8cd0ddbc8294392d78504d22cca19b1a532f899cbbe079599caf35478719d2af9f1fb767cdc5d7b031e14cab5d5f7205aa1b33c01cbfa1347eea2d8b7309bfb8f54cbc5764d87f5249885ab87918ed38a48a4a8440f17b9b799f07ff4f86b0ef5af6a9850f2cfe8282427873d94c7f5d93b7edbec8cdcdd6044ba08db4ae876236d16b6fccce61130b1f32d446d8d3e74de5d412be069dea6bfa3f55f15184acbde502cd55b5ab653146e933424bc85dd4af01c603a205fd2343aa7fed6867051e7905394bbb3699694ec6a6fd8b68f41ebf2060e8ea54e45e9c294c9121408128b920d980f2bf915b3781f1bec74afbf30958b20833806fc3d7982f982154bcfaa9d88a0f87a0df2a63dca46c9d4bed436cdf50e73967b4405c3cbdcb3ec2c1c0f872d1884cbb08992a2faaa829d590ed750a7783c36ef19c9a76a4bb4a5bd1b84f2eec26ad62f0d8279e42cf0e017785fc90aa0c72e734abd9614856e0ae86a49a0f4574fae547032741c741345f372a5c823b952dc95b10c13f142ef084d72661f33395c4ec53792f4d3b5cbcd67e1603c366aa8dd70dc3a7b3bbc81c0069eb7c69ece8dd744988c6754178bbb0ed8dc72cc621cedee47a202db52cec49cab2fe198325c2fd30b17bde4358eb25d7d573171b862c747a33c11df2e7bb5781e79dca58e181b8c2b116bd8383b15aa78c1732c8e6155fc3f095fb77e1dbd2ab9dfef26a936d489158e1d3dde3eb1d3f727548138d9faf5f55ced3b8e014051e58bd99f752d58e839999df3f60f903b768d2a1ff097769664acd034e6a0f3596985db17da232f29863267b10e0ed34c71ded167fb2b4feeec465ef6af0d17d248d3cc0e577e027c51a7d90eec4a90783f37b5e48cd1c474a590e790a548ff204bc775ac61c66777f0230d7f107d2ffedc6ef2b92dda1d7e79f40d5b85a2eaf623f543165260b8e67fb0b58ca46dca0c4fc048f8593c6223d0652f57bfa1e33d7923644afd4fb0c1e4028ced5c9e9296780570644225b20631d1c53fb904e932e7620256e06777bff2a89d7dff2c7fecb5a2d48ab2b9abdd8021938efe8e78f5ba13452100d87aee8d298a7e1a80b844dc6457be7c28f6715a7de70773bd3aa380d8969e105ccad6f22dce0ae344f0a57a4fd63dc9ecb4aa2eb17c9283cc701123ad448b994e3d5e8523329d31dda9af4dde215f7c35033f527074c1d746e0b4ff05d93996d0afb9892dfd7bb6ba7097bd8d4b234b6172b6948f7fc80e66c7bae3bb211b68676fa3d21152eb38f27cfbaa3b718b3a7462eb0d2ec7550b6046c52b70ee54d5035bdc4f8f93bf87cb2bc0a7b899d8d51c55527efea11917b0d09791961b564624f978a6881b5540b3792f07f93c180a856d7553f91079814590f1d76c2dc0089036a61d38c81a8b24e6a34da7483a1cc2174bcf133a651afc145b8702bd6a20ba888f54ac50da3d44a9cb1b7c2248cb5da3d41e60cd1ceeb5892f0cf72b25bc3e2559d98c068c02d26ed1d04e2f9bf89569675821f1feb4e4445125be19a354ad6f870a3b57fd0e69d04b90d622b338e22d447b3b5647c15d4282e8801f9f6dbab5f1cf297f1ab7230d04433a5c16bf06830f3160b04dce7b2a3ae4edc4dd63fbb6484133b6ad1f580456a7f6cb6b7128f4f52e6e066a8476ee181d9298f69e07e8a153023dc5f2f581d6caad44b63be0f87433a9d8aee7a68135bb683a1bc9fe169ea41bb8f9be123e8723cf3ba0abbfbd2d0ff15dbd374ce3a0e6a2b81e557584095530f1d05bcb2cffb079ed056ff9b4d67cfbce4af3a3d52f1c300621d7045b56010b8d01e539396079ab966126391263b722dab43f002516d4cbd890171d5f01852a3b5c4613086f9a1fe25389100f3e442a20a106cd9c73ebd5be2dfdc691a8d729bd56508be1d5edc02a2518758084f05d42059c1e352af53274baa8ba317a931c8452bbef8f04939343433afae01b9489fc10d5651c0e491265e0fa65f114503bddf462b216ac6e26000f44497e577377ee6e2d3b3af7c34654dc89adbcac132ee78c9d5166b5702ed5c5222b98e1c5176686e3d19a86c3b2eb1f95b1d2279f3ef11c865637f76dcc241a8e71e8c833c949746c348e403d463bd87c6c53eae605508480e108c5387975b6e98285c3035535c0efd3a3fc21957431f82717abfe03e71cd9a077f61cef2b28e7e6480b769b9c0f1a466b8201ce37fe51b13574c73da11cb5aea806a1438059b2ab42f912812f7b4ae4363a16688eebe733730da70c1854d8ad2ffdadbed118e91017aa516a49f19e9a68850f79c6f17d470198422ee1cdb6c1f5e900941f6a5a6757bb2f72c1183b7e2d6a6b4510e6a46969ed3e1a2e813513583d65c2f539844c4bd0e97a53b6ca45fc2d7156579366ab6ff9542888a56728404d014eb4019994dda53aee376e57719f081f939f85392e27fbe72a5cf65510d148270d1ed22c2f46863e325133d6a722fb55c5a900f7308e874317595ef4cafcd40fa844338344c0dc4e8e05218ad3565a50ab9143b63b333e360d1fbf3fc9fe341bd9403bc3013f3440039d923b2b6adc9e2435e8b9ffb78fce1b88109e13c948984ddc8924f5da1e77380f5a2f7ad81d31d047e6be45a774ca3082353dc9ef14c421b8129042d8ca11cb77806f3f93fdd6f537ae731f137728120b5750d69ca674545cf09d81f29b0f53e73373383befbb7ddaa39ceb45a84a0c268687fe307a3ad501e9bb0d19a5b223d67a8a32d6e3c2c3e090c06c92834b09634ece2fcf520a8337e4b28da57410b7084fdeb1f9b55b308b17164cb72e9503600ce2ca6c70bc32e4269bd87cdf22c1e45a6cda1bf658c98dedd878186fab3978cbe48184e02a05b39c4c286fe20158e581d3e2b5e4e5e2d438439125fe2e05d3f2fea69d0934c7e6eaac4e793ac5a7feaa4760ce7575677ca9ad9d1c8c803ba96e937d7cec429a770f68270f1813fd74dcde6956714650cf54b3ea24b8771389ecd3ee1ac0c4eaac2341918c819ed65af8671cd3bb58c0bb6c1a27db24f21686373dfe2274699c54c16c4986fb22d9a8caae2222be12d235c78a50a2489610cdf97c7761e0fad69c966ff8f1a41cd2307d1f4adbb094aa34eac0cbf8b6bf0aa60aef8314c37d3217bd36b7726c51bab1aa8705215b0d1fe973f59fcb8c9513e3310c32b88ae4156a292fa1d02acf6204555fd6b692f80346a100b3c2232b158a94d7558facb635b69dc5c87c7cf2d2495ade53fcdb50ae0163d4798749d62ecedef3919193ea4c0034403489d09c2f76e7f251dfacd60f9bb824579e6e983cfc448d8afb75088891e8bc5fa0854de7457c5810106751a7f412b12a995b8c29a4b6734a0477a373be1674007d969465f27e95c9a0a3c292a3b51272584ec802471323ef4b8e0b39cb34da4b7ae870728464dd73c5a7c5bfcf65e8afc800572be59b9f1b6f30a42215f75b56172a2ec29320288a666623fade667b4d84b16632c5972ee16fbd69fe81d194ce510cb48f4d47081ace37ee8b1531a90f9831da160cda269151dd7dbaeeeb7942c513c10a19ac9f38995f2cf49c1d3c03c909ca3a564458cc4e483790e9be82047fbf84084a97ad6c452c29e794b9046b69b93223281737979c23047f95c41d29dc552ad85c3db2e7da9003b9105e030dca309229e4e89a10fcb0f5456bc06283ccb3938a8e0d5958ce693911f3fcfb7e34d41219d1c7bea4683396e6ac547b62bf80f0a84e5381825baaab9fdf346a6a939d6538e535a028e1a0f9e545e11441f8bb717d67943de3f1ff11f212c0af480b6d6927b9a2631176b63536d489c1a8a4ba7892a84b393e5e8793458a12ca5f795651021ffdb236344adec8bf6012d22260727a44777b5685575724525ce1b82fd83b2689944cffc8388ae4527d7eaa6b7708ac6f5163c2d62fcc13f9830bafcfd3eb1e9bebe7ac1b1a2d38b2c0c387e0156a9fd2be9d8685bf7b4442960045bfa0571817c966921b723d5d783a7245b6f98c8bb5abc08c1938da45d66ca89cc8bf243c860e264cf5906a6d9545dbe3778c2552929b2d8989af3a92e5df28872f63aa0ba09355d01f2b5a0f2d7d4fc20454d9e4a9340f21589cf2b9ab88dc19fd6774205b3988b719ec5d74957952c0efa64fd425aaab5367b61d9634344a0f32f7a01d0e61db9ae1e240078293239712d52f741b8e1aef58c3a4fa9c0d40a154ec0e23a7f12fb438632d64282171fe705cbcca36d5c331b4cc446b6f215833d941bd3d0cb2c8b63d579388c59096983c93da17a55c9f58e0da42f4a29e88be7eeeaa1898b624352f9e93dc8ee1ef0297cbfe0a38d19d89bf572f26cf80dda465e7741067b75a0f6f5e2079753a1b106a67efb204a121a78c7017487860360b90ad1103dc9afcd1f5faed05acafcf5b776efc91b894b5a8895eea5217609b5935f669049513305aa10a9f62de1f824f550ddee657f1b0d7a338b19d18e4b3e26aec17217d0f58a5aa6e39a835f3443f5caf522988294d756b036bed298c4693a30c933867c4c92989c122e0d3da3c3358b219ce8e9b2e99fe2c6346b9cb581b86f85c2e33d2d34b405aca83ac915608732b7b258965a55beee0f9ea166f558ad9d06d07d4137a030772520dbddd3a057e6d2ac4d3079a34731c56de2f8291b59f05330912caf5ac13c25a1dc2131f90e17998f14f62cc70831f43afb4183ee498e77c0566d42997468fa83a53899df2b1b67a0fe11454033ce03f7ebc5002218c2f4e266da31796bb06c1f6aa1c3f879d5d21f9f81062bddb28c805050fe63a55789f1fc50dd67a11bbbfd870b53cc296d719cf403663303e3503547e323f10f74bc707648a3318a1c6abd0428c24df43fa0a208c67ad855dafb0fa115669664515dd2782a32098ef68850b7c2533d55af6560d248154148335dd84b2de3eaab3554c265ab235361b161ba52d79db25e24190ebe26795f78e4c1a97cf40284ad278f216dc27ed90c85eaa2b9cda7708c73fa45bd3792559207b2812b47da176282df71ca4e25dde19e9758e93b8dff9b4d6f6bb81abff143ad08c952e426a151db1f8326c37a54a3dee38de00dce18987c216dcb821fb76f06aaf596a1937b314a23535313017e5a3bd02bf3e6cfaa1e2643f0a26318abb70eed77fc9896e32d060693537069191a96db3b51b4978ed0826cb9f7a017396b99e6545b4c40c21e9c3f8215a584aa0c780ee42689f51a605631b9d7a9397734b9077d4fd5f00bcc1ecf13380598a8025195cb9cce565af0012f7d8063d9337088759fcd20101d6ffa8a3390b006d4583e3291b79e6cdf0813bcf88d9955f98d2371aa2b4d91dd555129714823cd9d7ac58695dbac826057b8a76818891c37395973987ce6a2be088eed92fdff7bd9852737f84461e304a96b8a258ac201fbadf618954278ff9912e8a2e2be415f7ae6af565cfc8724c8f4d0e1df9046d6bcd49bc3f77daaab84f24ad3530d703c16466ab20c208e3aa9722cc73da0d5107644f6fd0f4bc9bf40149584e239e543f24ce5765d23483a337e0ebf7ff6e1e00112108a5a50e838892b86a92226cd615ed8a5920c73f63d2129c853dfb9a54c1a7e5210850d5ae7c49243b423a5471d381acfe94300feba23b166859517f56f91cca5dbee19e15e87cf9aa4028b42ceea7efc56b1a6f828176ca63bd27aead72bbefa5312c7a4a6fec59dd1f291b0a58f650f3b6ce5f437519edcbfc7b58cdf83da36d3baf5762ed625b52411380acfac0109437710443ea4dd5042013d1b77c27c330521ada326fbe94a28003d7e270728d06eea4d652317d81d2845384c5f774b8906e2644764c3dc67acd3836ea045f046f5644b79a972d42864a0367d3e6d26112c88485d63ed3906fa336bba0a8300ab56ead40af49d04dc32fac383eb4a876534634197372b0f9001e0b9d01e6ee17484f3276907314b510f86b4b22be1cb84fa5dcdb67e7874b531e12ff6896e5041eba791f741151362c03ca66685b4e85f8cd38595e9d4855655426b23dbda6a50ec4b43091cdf2a6adc32797cb3af7e73404f40e158a06505feb155aa6e96dd4973aab68c0bdc2d9ae6dbd84b67d2742802d983eea7ed679328626cf4c477bcdef597da667d294cdb82a0a7e7fc6717839d1da8a8aec5e44e7123a1246f7fbddac2f5c48e69cefcf5eb2a628caa77b91d28be700985d261cca3c2048c35417c34204e035f07017c55176cd4bddb0b04d3f98a756f39856baa43e11e8777e4a9d1d79a5ac8c9413e913e56a0814314cf09ff1a51a02f4a8e51b800c998ed11b996a7828aa2fa1d384245fe097954a5ef75c6c19125f01027c1872240e3c585a3b9769d2c8d47a91a5721ae5503981b2fc02a7f0e6c203c195facd1544640eea2c354976f79acb57ae479c235522f30267c2bcf0eb186b8b0570620e38072041309f3ee79382af7fd1e087c987005210a594ac452cf4f8cd1befd81a844e7ed2d13963ef77be237210056e6251712ef6bf3cf686ef7ec62bcec5fd314a2f80e036167946645cd1d40399d4c78aa45e78cae99f9e7845a17e1162ad2dfc26426d830a1f5c0846cbb01d85cb2b5488727df8f4e77af49943bbdc85f412c1986315bfeb7f4b2761463f4e3992080ec64bc47485f5388c0c147a77998d344a51d60be16438a6986c65028e9a390691c9aef240f7f0514c0e1e9b3f2849006df8949ee45a5c5b2996fd1c497a7260a6cd5fce873e4d4185ca5d79da2ed7089cc1caadd75c9796595bfc3f6010b1817044e9b8123277441021827fe1c59c32c46359a2511dd1e4b751cfde29cbae57828a815764c127cc56d219020536fd3c41be7fecee24c5e7529117d3b6c70c751732fcb67f1b39e2a5e901288a6db820306b938ab21a0b808aa8f65e3c0b39905fd782f840beb3ff377dcbb46d5f75328823394fb6b8ff809fbd2eafe5be484c8abf5e30a84c70cef6bc175a343a2d14fbf531fadf8dadece387959a3a433a98316a81aa3ca1242a2d19556fed88d912e6749c8a941f20163165d0a1c1aa81d46012025a31156db63f8a1e3573bcdd1d5bd67413f9432ab50b741addfbec64b4f14cfd59d95a5b60f5b9167951676614f14b00472f4b4c8cfe65da0da7ac73810049a9c3609cddaf50f089df3986df1a51830004586351673cc9a9ebccd1edf71b0597b6369a34187bcd4aed53b994051a2b1c26f2ad6425f7cb22a6630739da814db1069f929a884173eeab9ec6f818fedd22eb828161978ab7b2a3f7396f8d23536ca2cfa4e93efd1224d7efa5c139cea99d89cf19c75aa3b305b783f58df099ed0ea64466031d764e3494deff2e3929f94dcd62b4eaf989d926fdc5bcedb7251b0af356a340c5e2564dfce9534723aef055882a88c0237455b2a26f6eadfe627ca776ecf208225730aed3f25d621a3c8ffb500ec5e2c523928ee49905486cc7cbc915b2519abadad770bc7e6e2f1712d07766b4d73a0f1b751de3bc2eb41b9b879c755af9e1b102569fbcb3d71a0058232dd51c1b9aa70dd23e82a9d5227df6404b9817460b636e3ef720eb045b370fd7b88c399710f78741dede33ee4d1a19c28f4ba82302fef4a7b167346ec03fab6e8eb489dfe5e2a85f9dc9da42b047283bdc1d1082ca2649a6d2fe889e9639e9c0f74f59aba9180aa9f9ef3d2ae4834307f2239794cb372f67d8e17046841ce83a8713bbe43b8a7bfb508421a3d6d49631407663e4eb909fdf39fcc166ec13346a4259cfc07bd4c2fe00861c9eebc78076d9f5bb8757f2fad939b493f0d81f373cc09e74f1db62fff487251ac0474babb4a42ba36b63bb77e107f4dc45fd33f40c00f0c0b043b480f15fa37c9397a3ade86d168e33c18c8d7d77e8f179d02986f402bc28f15f3715b17d2f932bebda8e0abb2176e4339a41c15b4fcd109f328ef71eff50ba94e591c12c4c197b006fa684f88725551920a8a419cc745b9a1b3594d80af9f6e5a92531a858ad72b5f0a0380bb3b3afab7249ac66769a5a083766ad5f00a21bd095874bb06e8183debb71f328eab7d538ed47601abd2cb4e0110229c450cbc0a3dacf77ca1be7d4c60f526564ffdbb13a0dfb03a3167be9091c282121905f21470f9b811bd730383ea1a28be7d048fea316d328e82e87917c0fa581b0c5880af1bcda5ecde40d116050a9bb2d1c5bbcac03cd65fff023558e0a99ef8f8e427c6b20885adb650cd19c3c7588298469ffe94212196e7cd2ca420f690f9158bbaf204f47e3c5ab0a37234946b7e482457fb6dfc188b864760b590c918d619e987faca0768aa466c274b3a5e1f16a37686487d147ba6ab1ba2b8d9be1fb26fe4f5baa94117d1f081da9adb3a193825630343a32fac6678b4a90b02f15669de1c26112fecb97b2c1d5d2fdad12aa4bb54a4b8c5c17c6970c0aef5a34c4a5760b8bc9071270de9ba3a015db8f8ed5e899b68a150720cefba8ea62bf30437a5a3e88eb76cd486924e5a22d8f93e0f281412a91e713dbcdf954fc6e8083eab4fa2e062ad815bb00166ad9fa66a6966f277dae5a5d920632659f727cd803692cdf439b5bd91dfb5102ca002ee0c045c83d2c001d1671c8375c1c72b9e02b6cecc1851f150056c7a0c280b711b68a376072d7ef7b42b140a6cc72677d789c8b3a72b93eb246669ceb58d993cc1b9042270625301b2cc84efb2b87a688b922bc83780d33db8c017258c49a0645156814bbfb910a158110bbe33b101932e985a4d39637487ed1f59cbd1419e6001f6be2065e981369388da39d917a2524bfa4261babd5dbc06c932639e5937c7f8e6ff88515f10115b1494ca4b74ca38be6be72bd0329ed8db64d22f612fa91eef917e528afce83866ce743784ece16ef98016a1fb86b358df345eecadc7a2dde8301f73c3633456cc55c0f606afd7680a98cf6cca852a43a0ba9b525f52fe9713f470a62e3902683f0757eee89bf16258a28a59e028c48378918550842acaf298c19956deaab130a625b771a92ba2d7ffe36750308e61f0b5ae4e2de3d4da9d4f53fb0e60524532574ac28fae9b2712c00884e68a6b561f75ca4bca149bcac141fd3be1ef0008356874eaf92c1f3abe032135f55569e8eafa1c1ad617b3d26a337e5daaf041f5c419f2e6c5985b686790c95ec70f70baa822b70e47036c51d10e80f2e3bc76f9cdbccdb649efd57612ba537f519beaf6cca54585e71542e1d355588255a95709f53c8e1025da0ddb2050ea0b8cbe6b32ad297867b2e362077aa1012c0344333cfb5ba48cda7469e78984eae21c4454786676a89f2ee997a8ec1e173f8422f3a98602882218ffdd1d3476c19f550b84e72993dbd2b4aac4bc804ddb5b61f5e3887f6d6db594bfe845296d50ee33e3f35b677dc781bde42be674442413f51447f8bc3886bb41d8504a9fed33db491bcc0275dede8e0fd1d9ce23bd83424ed8a93d3b91406830924d2a796a03b8a75ec16d80cbc4348a4b5baa26a8287414f74de464c1718d8bc6c4f3c15b31259f5e00f7a96539e173a0390bbbdbe84c5262a8e39a2fb44c02bdfadc4c392270863fbbf3855bb62273a2268b5c168b1bafb55ba4a086243efeeaddc98c6a907cd2f36b62e83656b05c44d943e9f0c8855c386fe09a65e5f9d6eddeaf0436e1e730d6a94feb3ac4a42f06517a11b25eeb97a8562f81ca354455adf40415ed901";
        let tx_zcash_bytes = hex::decode(tx_zcash_hex).unwrap();

        let btc_tx = bytes_to_btc_transaction(&tx_zcash_bytes);
        println!("ZCash tx: {:?}", btc_tx);

        let output_script_pubkey = btc_tx.output()[0].script_pubkey.clone();
        let deposit_msg = DepositMsg {
            recipient_id: "omni_user_account-20250625-153431.testnet".parse().unwrap(),
            post_actions: None,
            extra_msg: None,
        };

        let path = get_deposit_path(&deposit_msg);
        println!("{:?}", path);
        let deposit_address = generate_utxo_chain_address(&path);
        let expected_script_pubkey = deposit_address.script_pubkey();

        println!("Deposit address: {:?}", deposit_address);
        println!("Deposit address: {:?}", deposit_address.to_string());
        println!(
            "Expected script pubkey: {}",
            expected_script_pubkey.to_hex_string()
        );
        println!(
            "Output script pubkey: {}",
            output_script_pubkey.to_hex_string()
        );
        assert_eq!(expected_script_pubkey, output_script_pubkey);
    }
}
