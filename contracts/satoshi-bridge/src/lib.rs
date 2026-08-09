use near_plugins::{access_control, AccessControlRole, AccessControllable, Pausable, Upgradable};
use near_sdk::{
    assert_one_yocto,
    borsh::{BorshDeserialize, BorshSerialize},
    env, ext_contract, is_promise_success,
    json_types::U128,
    log, near, require,
    serde::{Deserialize, Serialize},
    serde_json::{self, json, Value},
    store::{IterableMap, IterableSet, LazyOption, LookupSet},
    AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise, PromiseOrValue, PublicKey,
    Timestamp,
};
use omni_utils::macros::trusted_relayer;
use std::collections::{HashMap, HashSet};

use bitcoin::{Amount, OutPoint, PublicKey as BtcPublicKey, ScriptBuf, TxOut};

pub mod account;
pub mod api;
#[cfg(not(feature = "zcash"))]
pub mod bitcoin_utils;
#[cfg(feature = "zcash")]
pub mod zcash_utils;

pub mod block_amount_ring;
pub mod btc_light_client;
pub mod btc_pending_info;
pub mod chain_signature;
pub mod config;
pub mod deposit_msg;
pub mod event;
pub mod json_utils;
pub mod kdf;
pub mod legacy;
pub mod nbtc;
pub mod network;
pub mod psbt;
pub mod rbf;
pub mod refund;
pub mod token_transfer;
#[cfg(test)]
mod unit;
pub mod upgrade;
pub mod utils;
pub mod utxo;

pub use crate::account::*;
pub use crate::api::*;
pub use crate::block_amount_ring::BlockAmountRing;
pub use crate::btc_pending_info::*;
pub use crate::chain_signature::*;
pub use crate::config::*;
pub use crate::deposit_msg::*;
pub use crate::event::*;
pub use crate::json_utils::*;
pub use crate::legacy::*;
pub use crate::nbtc::*;
pub use crate::rbf::*;
pub use crate::refund::*;
pub use crate::token_transfer::*;
pub use crate::utils::*;
pub use crate::utxo::*;

#[cfg(not(feature = "zcash"))]
pub use crate::bitcoin_utils::psbt_wrapper;
#[cfg(not(feature = "zcash"))]
pub use crate::bitcoin_utils::transaction::Transaction as WrappedTransaction;
#[cfg(not(feature = "zcash"))]
use crate::bitcoin_utils::types::ChainSpecificData;

#[cfg(feature = "zcash")]
pub use crate::zcash_utils::contract_methods::*;
#[cfg(feature = "zcash")]
pub use crate::zcash_utils::psbt_wrapper;
#[cfg(feature = "zcash")]
pub use crate::zcash_utils::transaction::Transaction as WrappedTransaction;
#[cfg(feature = "zcash")]
use crate::zcash_utils::types::ChainSpecificData;

#[cfg(test)]
pub use unit::*;

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Accounts,
    BTCPendingInfos,
    RbfTxs,
    Config,
    UTXOs,
    UnavailableUTXOs,
    VerifiedDepositUtxos,
    RelayerWhiteList,
    PostActionReceiverIdWhiteListWhiteList,
    LostFound,
    PostActionMsgTemplates,
    ExtraMsgRelayerWhiteList,
    PendingTxLimits,
    RefundRequests,
}

#[derive(AccessControlRole, Deserialize, Serialize, Copy, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum Role {
    DAO,
    Operator,
    PauseManager,
    UpgradableCodeStager,
    UpgradableCodeDeployer,
    UnrestrictedRelayer,
    RelayerManager,
    RefundOperator,
    UnpauseManager,
    MigrationOperator,
}

/// Transaction inclusion proof with coinbase verification (v2).
#[near(serializers = [json])]
pub struct TxInclusionProof {
    pub tx_block_blockhash: String,
    pub tx_index: u64,
    pub merkle_proof: Vec<String>,
    pub coinbase_tx_id: String,
    pub coinbase_merkle_proof: Vec<String>,
}

#[near(serializers = [borsh])]
pub struct ContractData {
    pub config: LazyOption<Config>,
    pub accounts: IterableMap<AccountId, VAccount>,
    pub utxos: IterableMap<String, VUTXO>,
    pub unavailable_utxos: IterableMap<String, VUTXO>,
    pub verified_deposit_utxo: LookupSet<String>,
    pub btc_pending_infos: IterableMap<String, VBTCPendingInfo>,
    pub rbf_txs: IterableMap<String, HashSet<String>>,
    pub relayer_white_list: IterableSet<AccountId>,
    pub extra_msg_relayer_white_list: IterableSet<AccountId>,
    pub post_action_receiver_id_white_list: IterableSet<AccountId>,
    pub post_action_msg_templates: IterableMap<AccountId, HashSet<String>>,
    pub pending_tx_limits: IterableMap<AccountId, u32>,
    pub lost_found: IterableMap<AccountId, u128>,
    pub acc_collected_protocol_fee: u128,
    pub cur_available_protocol_fee: u128,
    pub acc_claimed_protocol_fee: u128,
    pub cur_reserved_protocol_fee: u128,
    pub acc_protocol_fee_for_gas: u128,
    pub refund_requests: IterableMap<String, VRefundRequest>,
    pub block_bridge_amounts: BlockAmountRing,
}

/// Borsh encodes enum variants by index, so the placeholder variants must stay:
/// they pin `V6` and `Current` to on-chain tags 6 and 7. Tags 0..=5 belong to
/// pre-V6 layouts that are no longer deployed anywhere.
#[near(serializers = [borsh])]
pub enum VersionedContractData {
    RemovedV0,
    RemovedV1,
    RemovedV2,
    RemovedV3,
    RemovedV4,
    RemovedV5,
    V6(ContractDataV6),
    Current(ContractData),
}

#[near(contract_state)]
#[derive(Pausable, Upgradable, PanicOnDefault)]
#[access_control(role_type(Role))]
#[pausable(pause_roles(Role::PauseManager), unpause_roles(Role::UnpauseManager))]
#[upgradable(access_control_roles(
    code_stagers(Role::UpgradableCodeStager, Role::DAO),
    code_deployers(Role::UpgradableCodeDeployer, Role::DAO),
    duration_initializers(Role::DAO),
    duration_update_stagers(Role::DAO),
    duration_update_appliers(Role::DAO),
))]
pub struct Contract {
    data: VersionedContractData,
}

#[trusted_relayer(
    bypass_roles(Role::DAO, Role::UnrestrictedRelayer),
    manager_roles(Role::DAO, Role::RelayerManager),
    config_roles(Role::DAO)
)]
#[near]
impl Contract {
    #[init]
    pub fn new(config: Config) -> Self {
        config.assert_valid();
        require!(
            config.chain_signatures_root_public_key.is_none(),
            "Init chain_signatures_root_public_key must be None"
        );
        require!(
            config.change_address.is_none(),
            "Init change_address must be None"
        );
        let block_bridge_amounts = BlockAmountRing::new(BlockAmountRing::capacity_for(&config));
        let mut contract = Self {
            data: VersionedContractData::Current(ContractData {
                config: LazyOption::new(StorageKey::Config, Some(config)),
                accounts: IterableMap::new(StorageKey::Accounts),
                utxos: IterableMap::new(StorageKey::UTXOs),
                unavailable_utxos: IterableMap::new(StorageKey::UnavailableUTXOs),
                verified_deposit_utxo: LookupSet::new(StorageKey::VerifiedDepositUtxos),
                btc_pending_infos: IterableMap::new(StorageKey::BTCPendingInfos),
                rbf_txs: IterableMap::new(StorageKey::RbfTxs),
                relayer_white_list: IterableSet::new(StorageKey::RelayerWhiteList),
                extra_msg_relayer_white_list: IterableSet::new(
                    StorageKey::ExtraMsgRelayerWhiteList,
                ),
                post_action_receiver_id_white_list: IterableSet::new(
                    StorageKey::PostActionReceiverIdWhiteListWhiteList,
                ),
                post_action_msg_templates: IterableMap::new(StorageKey::PostActionMsgTemplates),
                pending_tx_limits: IterableMap::new(StorageKey::PendingTxLimits),
                lost_found: IterableMap::new(StorageKey::LostFound),
                refund_requests: IterableMap::new(StorageKey::RefundRequests),
                block_bridge_amounts,
                acc_collected_protocol_fee: 0,
                cur_available_protocol_fee: 0,
                acc_claimed_protocol_fee: 0,
                cur_reserved_protocol_fee: 0,
                acc_protocol_fee_for_gas: 0,
            }),
        };
        contract.acl_init_super_admin(env::predecessor_account_id());
        contract.acl_grant_role(Role::DAO.into(), env::predecessor_account_id());
        contract.acl_grant_role(Role::PauseManager.into(), env::predecessor_account_id());
        contract.acl_grant_role(Role::UnpauseManager.into(), env::predecessor_account_id());

        contract.internal_set_account(
            &env::predecessor_account_id(),
            Account::new(&env::predecessor_account_id()),
        );
        contract
    }
}

impl Contract {
    #[allow(unreachable_patterns)]
    fn data(&self) -> &ContractData {
        match &self.data {
            VersionedContractData::Current(data) => data,
            _ => unimplemented!(),
        }
    }

    #[allow(unreachable_patterns)]
    fn data_mut(&mut self) -> &mut ContractData {
        match &mut self.data {
            VersionedContractData::Current(data) => data,
            _ => unimplemented!(),
        }
    }
}
