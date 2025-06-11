use std::borrow::{Borrow, BorrowMut};

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

    pub fn get_psbt(&self) -> Psbt {
        to_psbt(&self.psbt_hex)
    }

    pub fn get_transaction(&self) -> BtcTransaction {
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

pub fn bytes_to_btc_transaction(tx_bytes: &[u8]) -> BtcTransaction {
    deserialize(tx_bytes).expect("Deserialization tx_bytes failed")
}

pub fn to_psbt(psbt_hex: &String) -> Psbt {
    let psbt_bytes = hex::decode(psbt_hex).unwrap();
    Psbt::deserialize(&psbt_bytes).expect("ERR_INVALID_PSBT_HEX")
}
