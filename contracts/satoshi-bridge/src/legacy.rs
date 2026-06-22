#[cfg(not(feature = "zcash"))]
use crate::VRefundRequest;
use crate::{
    env, near, AccountId, BlockAmountRing, BridgeFee, Config, ContractData, HashMap, HashSet,
    IterableMap, IterableSet, LazyOption, LookupSet, PublicKey, StorageKey, VAccount,
    VBTCPendingInfo, VUTXO,
};

#[near(serializers = [borsh])]
pub struct ContractDataV0 {
    pub config: LazyOption<Config>,
    pub accounts: IterableMap<AccountId, VAccount>,
    pub utxos: IterableMap<String, VUTXO>,
    pub unavailable_utxos: IterableMap<String, VUTXO>,
    pub verified_deposit_utxo: LookupSet<String>,
    pub btc_pending_infos: IterableMap<String, VBTCPendingInfo>,
    pub rbf_txs: IterableMap<String, HashSet<String>>,
    pub relayer_white_list: IterableSet<AccountId>,
    pub post_action_receiver_id_white_list: IterableSet<AccountId>,
    pub lost_found: IterableMap<AccountId, u128>,
    pub acc_collected_protocol_fee: u128,
    pub cur_available_protocol_fee: u128,
    pub acc_claimed_protocol_fee: u128,
    pub cur_reserved_protocol_fee: u128,
    pub acc_protocol_fee_for_gas: u128,
}

impl From<ContractDataV0> for ContractData {
    fn from(c: ContractDataV0) -> Self {
        let ContractDataV0 {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            post_action_receiver_id_white_list,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
        } = c;

        let ring_capacity = BlockAmountRing::capacity_for(
            config
                .get()
                .as_ref()
                .expect("ContractDataV0: config missing"),
        );
        Self {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list: IterableSet::new(StorageKey::ExtraMsgRelayerWhiteList),
            post_action_receiver_id_white_list,
            post_action_msg_templates: IterableMap::new(StorageKey::PostActionMsgTemplates),
            pending_tx_limits: IterableMap::new(StorageKey::PendingTxLimits),
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
            block_bridge_amounts: BlockAmountRing::new(ring_capacity),
        }
    }
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct ConfigV0 {
    // The account id of btc light client contract
    pub btc_light_client_account_id: AccountId,
    // The account id of nbtc contract
    pub nbtc_account_id: AccountId,
    // The account id of chain signatures contract
    pub chain_signatures_account_id: AccountId,
    // The root public key of chain signatures contract
    pub chain_signatures_root_public_key: Option<PublicKey>,
    // The change address of BTC transaction
    pub change_address: Option<String>,
    // Satoshi upper limit for amount checks -> confirmations
    pub confirmations_strategy: HashMap<String, u8>,
    // The number of confirmations that need to be increased when a relayer not on the whitelist performs a verify.
    pub confirmations_delta: u8,
    // Used to calculate the deposit fee.
    pub deposit_bridge_fee: BridgeFee,
    // Used to calculate the withdraw fee.
    pub withdraw_bridge_fee: BridgeFee,
    // The min amount must be met during verify_deposit, otherwise NBTC will not be minted for the user.
    pub min_deposit_amount: u128,
    // The minimum amount allowed for the user to withdraw.
    pub min_withdraw_amount: u128,
    // The minimum value requirement that change address must satisfy in BTC transaction.
    pub min_change_amount: u128,
    // Used to limit the maximum value of change in specific situations.
    pub max_change_amount: u128,
    // The min gas fee applicable for Bitcoin transactions
    pub min_btc_gas_fee: u128,
    // The max gas fee applicable for Bitcoin transactions
    pub max_btc_gas_fee: u128,
    // The maximum number of inputs that can be used for a Withdraw.
    pub max_withdrawal_input_number: u8,
    // The maximum amount of change allowed during a Withdraw.
    pub max_change_number: u8,
    // The maximum number of inputs allowed during active UTXO management.
    pub max_active_utxo_management_input_number: u8,
    // The maximum number of outputs allowed during active UTXO management.
    pub max_active_utxo_management_output_number: u8,
    // When the number of UTXOs in the protocol is less than this configuration, UTXO management can be actively initiated.
    // The number of inputs in the managed PSBT must be less than the number of outputs.
    pub active_management_lower_limit: u32,
    // When the number of UTXOs in the protocol is greater than this configuration, UTXO management can be actively initiated.
    // The number of inputs in the managed PSBT must be greater than the number of outputs.
    pub active_management_upper_limit: u32,
    // When the number of UTXOs in the protocol is less than this configuration, passive UTXO management will be triggered,
    // requiring that the number of inputs must be less than the number of changes.
    pub passive_management_lower_limit: u32,
    // When the number of UTXOs in the protocol is greater than this configuration, passive UTXO management will be triggered,
    // requiring that the number of inputs must be greater than the number of changes.
    pub passive_management_upper_limit: u32,
    // The maximum number of transactions allowed to initiate RBF
    pub rbf_num_limit: u8,
    // If the transaction exceeds this configuration and has not been verified, the protocol will be allowed to cancel the transaction.
    pub max_btc_tx_pending_sec: u32,
}

impl From<ConfigV0> for Config {
    fn from(c: ConfigV0) -> Self {
        let ConfigV0 {
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
        } = c;

        let chain = if env::current_account_id().as_str().ends_with(".testnet") {
            crate::network::Chain::BitcoinTestnet
        } else {
            crate::network::Chain::BitcoinMainnet
        };

        Self {
            chain,
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            extra_msg_confirmations_delta: 1,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
            unhealthy_utxo_amount: 1000,
            refund_timelock_sec: crate::config::DEFAULT_REFUND_TIMELOCK_SEC,
            unsafe_refund_timelock_sec: crate::config::DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC,
            #[cfg(feature = "zcash")]
            expiry_height_gap: 1000,
        }
    }
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct ConfigV1 {
    // The account id of btc light client contract
    pub btc_light_client_account_id: AccountId,
    // The account id of nbtc contract
    pub nbtc_account_id: AccountId,
    // The account id of chain signatures contract
    pub chain_signatures_account_id: AccountId,
    // The root public key of chain signatures contract
    pub chain_signatures_root_public_key: Option<PublicKey>,
    // The change address of BTC transaction
    pub change_address: Option<String>,
    // Satoshi upper limit for amount checks -> confirmations
    pub confirmations_strategy: HashMap<String, u8>,
    // The number of confirmations that need to be increased when a relayer not on the whitelist performs a verify.
    pub confirmations_delta: u8,
    // The number of confirmations that need to be increased when a relayer not on the extra msg whitelist performs a verify.
    pub extra_msg_confirmations_delta: u8,
    // Used to calculate the deposit fee.
    pub deposit_bridge_fee: BridgeFee,
    // Used to calculate the withdraw fee.
    pub withdraw_bridge_fee: BridgeFee,
    // The min amount must be met during verify_deposit, otherwise NBTC will not be minted for the user.
    pub min_deposit_amount: u128,
    // The minimum amount allowed for the user to withdraw.
    pub min_withdraw_amount: u128,
    // The minimum value requirement that change address must satisfy in BTC transaction.
    pub min_change_amount: u128,
    // Used to limit the maximum value of change in specific situations.
    pub max_change_amount: u128,
    // The min gas fee applicable for Bitcoin transactions
    pub min_btc_gas_fee: u128,
    // The max gas fee applicable for Bitcoin transactions
    pub max_btc_gas_fee: u128,
    // The maximum number of inputs that can be used for a Withdraw.
    pub max_withdrawal_input_number: u8,
    // The maximum amount of change allowed during a Withdraw.
    pub max_change_number: u8,
    // The maximum number of inputs allowed during active UTXO management.
    pub max_active_utxo_management_input_number: u8,
    // The maximum number of outputs allowed during active UTXO management.
    pub max_active_utxo_management_output_number: u8,
    // When the number of UTXOs in the protocol is less than this configuration, UTXO management can be actively initiated.
    // The number of inputs in the managed PSBT must be less than the number of outputs.
    pub active_management_lower_limit: u32,
    // When the number of UTXOs in the protocol is greater than this configuration, UTXO management can be actively initiated.
    // The number of inputs in the managed PSBT must be greater than the number of outputs.
    pub active_management_upper_limit: u32,
    // When the number of UTXOs in the protocol is less than this configuration, passive UTXO management will be triggered,
    // requiring that the number of inputs must be less than the number of changes.
    pub passive_management_lower_limit: u32,
    // When the number of UTXOs in the protocol is greater than this configuration, passive UTXO management will be triggered,
    // requiring that the number of inputs must be greater than the number of changes.
    pub passive_management_upper_limit: u32,
    // The maximum number of transactions allowed to initiate RBF
    pub rbf_num_limit: u8,
    // If the transaction exceeds this configuration and has not been verified, the protocol will be allowed to cancel the transaction.
    pub max_btc_tx_pending_sec: u32,
    // UTXOs less than or equal to this amount are allowed to be merged through active management.
    pub unhealthy_utxo_amount: u64,
    #[cfg(feature = "zcash")]
    pub expiry_height_gap: u32,
}

impl From<ConfigV1> for Config {
    fn from(c: ConfigV1) -> Self {
        let ConfigV1 {
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            extra_msg_confirmations_delta,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
            unhealthy_utxo_amount,
            #[cfg(feature = "zcash")]
            expiry_height_gap,
        } = c;

        let chain = if env::current_account_id().as_str().ends_with(".testnet") {
            crate::network::Chain::BitcoinTestnet
        } else {
            crate::network::Chain::BitcoinMainnet
        };

        Self {
            chain,
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            extra_msg_confirmations_delta,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
            unhealthy_utxo_amount,
            refund_timelock_sec: crate::config::DEFAULT_REFUND_TIMELOCK_SEC,
            unsafe_refund_timelock_sec: crate::config::DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC,
            #[cfg(feature = "zcash")]
            expiry_height_gap,
        }
    }
}

#[near(serializers = [borsh])]
pub struct ContractDataV1 {
    pub config: LazyOption<ConfigV0>,
    pub accounts: IterableMap<AccountId, VAccount>,
    pub utxos: IterableMap<String, VUTXO>,
    pub unavailable_utxos: IterableMap<String, VUTXO>,
    pub verified_deposit_utxo: LookupSet<String>,
    pub btc_pending_infos: IterableMap<String, VBTCPendingInfo>,
    pub rbf_txs: IterableMap<String, HashSet<String>>,
    pub relayer_white_list: IterableSet<AccountId>,
    pub post_action_receiver_id_white_list: IterableSet<AccountId>,
    pub post_action_msg_templates: IterableMap<AccountId, HashSet<String>>,
    pub lost_found: IterableMap<AccountId, u128>,
    pub acc_collected_protocol_fee: u128,
    pub cur_available_protocol_fee: u128,
    pub acc_claimed_protocol_fee: u128,
    pub cur_reserved_protocol_fee: u128,
    pub acc_protocol_fee_for_gas: u128,
}

impl From<ContractDataV1> for ContractData {
    fn from(c: ContractDataV1) -> Self {
        let ContractDataV1 {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
        } = c;
        let config_v0 = config.get().clone().unwrap();
        let new_config: Config = config_v0.into();
        let ring_capacity = BlockAmountRing::capacity_for(&new_config);
        Self {
            config: LazyOption::new(StorageKey::Config, Some(new_config)),
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list: IterableSet::new(StorageKey::ExtraMsgRelayerWhiteList),
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            pending_tx_limits: IterableMap::new(StorageKey::PendingTxLimits),
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
            block_bridge_amounts: BlockAmountRing::new(ring_capacity),
        }
    }
}

#[near(serializers = [borsh])]
pub struct ContractDataV2 {
    #[cfg(feature = "zcash")]
    pub config: LazyOption<ConfigV2>,
    #[cfg(not(feature = "zcash"))]
    pub config: LazyOption<ConfigV1>,
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
    pub lost_found: IterableMap<AccountId, u128>,
    pub acc_collected_protocol_fee: u128,
    pub cur_available_protocol_fee: u128,
    pub acc_claimed_protocol_fee: u128,
    pub cur_reserved_protocol_fee: u128,
    pub acc_protocol_fee_for_gas: u128,
}

impl From<ContractDataV2> for ContractData {
    fn from(c: ContractDataV2) -> Self {
        let ContractDataV2 {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
        } = c;

        let new_config: Config = config.get().clone().unwrap().into();
        let ring_capacity = BlockAmountRing::capacity_for(&new_config);
        Self {
            config: LazyOption::new(StorageKey::Config, Some(new_config)),
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            pending_tx_limits: IterableMap::new(StorageKey::PendingTxLimits),
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
            block_bridge_amounts: BlockAmountRing::new(ring_capacity),
        }
    }
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct ConfigV2 {
    pub chain: crate::network::Chain,
    pub btc_light_client_account_id: AccountId,
    pub nbtc_account_id: AccountId,
    pub chain_signatures_account_id: AccountId,
    pub chain_signatures_root_public_key: Option<PublicKey>,
    pub change_address: Option<String>,
    pub confirmations_strategy: HashMap<String, u8>,
    pub confirmations_delta: u8,
    pub extra_msg_confirmations_delta: u8,
    pub deposit_bridge_fee: BridgeFee,
    pub withdraw_bridge_fee: BridgeFee,
    pub min_deposit_amount: u128,
    pub min_withdraw_amount: u128,
    pub min_change_amount: u128,
    pub max_change_amount: u128,
    pub min_btc_gas_fee: u128,
    pub max_btc_gas_fee: u128,
    pub max_withdrawal_input_number: u8,
    pub max_change_number: u8,
    pub max_active_utxo_management_input_number: u8,
    pub max_active_utxo_management_output_number: u8,
    pub active_management_lower_limit: u32,
    pub active_management_upper_limit: u32,
    pub passive_management_lower_limit: u32,
    pub passive_management_upper_limit: u32,
    pub rbf_num_limit: u8,
    pub max_btc_tx_pending_sec: u32,
    pub unhealthy_utxo_amount: u64,
    #[cfg(feature = "zcash")]
    pub expiry_height_gap: u32,
}

impl From<ConfigV2> for Config {
    fn from(c: ConfigV2) -> Self {
        let ConfigV2 {
            chain,
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            extra_msg_confirmations_delta,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
            unhealthy_utxo_amount,
            #[cfg(feature = "zcash")]
            expiry_height_gap,
        } = c;

        Self {
            chain,
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            extra_msg_confirmations_delta,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
            unhealthy_utxo_amount,
            refund_timelock_sec: crate::config::DEFAULT_REFUND_TIMELOCK_SEC,
            unsafe_refund_timelock_sec: crate::config::DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC,
            #[cfg(feature = "zcash")]
            expiry_height_gap,
        }
    }
}

#[near(serializers = [borsh])]
pub struct ContractDataV3 {
    pub config: LazyOption<ConfigV2>,
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
    pub lost_found: IterableMap<AccountId, u128>,
    pub acc_collected_protocol_fee: u128,
    pub cur_available_protocol_fee: u128,
    pub acc_claimed_protocol_fee: u128,
    pub cur_reserved_protocol_fee: u128,
    pub acc_protocol_fee_for_gas: u128,
}

impl From<ContractDataV3> for ContractData {
    fn from(c: ContractDataV3) -> Self {
        let ContractDataV3 {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
        } = c;

        let new_config: Config = config.get().clone().unwrap().into();
        let ring_capacity = BlockAmountRing::capacity_for(&new_config);
        Self {
            config: LazyOption::new(StorageKey::Config, Some(new_config)),
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            pending_tx_limits: IterableMap::new(StorageKey::PendingTxLimits),
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
            block_bridge_amounts: BlockAmountRing::new(ring_capacity),
        }
    }
}

// Snapshot of the v0.8.0 `Config`, kept so contracts deployed at that version
// can still be deserialized after `unsafe_refund_timelock_sec` was added.
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct ConfigV3 {
    pub chain: crate::network::Chain,
    pub btc_light_client_account_id: AccountId,
    pub nbtc_account_id: AccountId,
    pub chain_signatures_account_id: AccountId,
    pub chain_signatures_root_public_key: Option<PublicKey>,
    pub change_address: Option<String>,
    pub confirmations_strategy: HashMap<String, u8>,
    pub confirmations_delta: u8,
    pub extra_msg_confirmations_delta: u8,
    pub deposit_bridge_fee: BridgeFee,
    pub withdraw_bridge_fee: BridgeFee,
    pub min_deposit_amount: u128,
    pub min_withdraw_amount: u128,
    pub min_change_amount: u128,
    pub max_change_amount: u128,
    pub min_btc_gas_fee: u128,
    pub max_btc_gas_fee: u128,
    pub max_withdrawal_input_number: u8,
    pub max_change_number: u8,
    pub max_active_utxo_management_input_number: u8,
    pub max_active_utxo_management_output_number: u8,
    pub active_management_lower_limit: u32,
    pub active_management_upper_limit: u32,
    pub passive_management_lower_limit: u32,
    pub passive_management_upper_limit: u32,
    pub rbf_num_limit: u8,
    pub max_btc_tx_pending_sec: u32,
    pub unhealthy_utxo_amount: u64,
    pub refund_timelock_sec: u64,
    #[cfg(feature = "zcash")]
    pub expiry_height_gap: u32,
}

impl From<ConfigV3> for Config {
    fn from(c: ConfigV3) -> Self {
        let ConfigV3 {
            chain,
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            extra_msg_confirmations_delta,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
            unhealthy_utxo_amount,
            refund_timelock_sec,
            #[cfg(feature = "zcash")]
            expiry_height_gap,
        } = c;

        Self {
            chain,
            btc_light_client_account_id,
            nbtc_account_id,
            chain_signatures_account_id,
            chain_signatures_root_public_key,
            change_address,
            confirmations_strategy,
            confirmations_delta,
            extra_msg_confirmations_delta,
            deposit_bridge_fee,
            withdraw_bridge_fee,
            min_deposit_amount,
            min_withdraw_amount,
            min_change_amount,
            max_change_amount,
            min_btc_gas_fee,
            max_btc_gas_fee,
            max_withdrawal_input_number,
            max_change_number,
            max_active_utxo_management_input_number,
            max_active_utxo_management_output_number,
            active_management_lower_limit,
            active_management_upper_limit,
            passive_management_lower_limit,
            passive_management_upper_limit,
            rbf_num_limit,
            max_btc_tx_pending_sec,
            unhealthy_utxo_amount,
            refund_timelock_sec,
            unsafe_refund_timelock_sec: crate::config::DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC,
            #[cfg(feature = "zcash")]
            expiry_height_gap,
        }
    }
}

// Snapshot of the v0.8.0 `ContractData`. Identical to the current layout except
// `config` references `ConfigV3` (without `unsafe_refund_timelock_sec`).
#[near(serializers = [borsh])]
pub struct ContractDataV4 {
    pub config: LazyOption<ConfigV3>,
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
    #[cfg(not(feature = "zcash"))]
    pub refund_requests: IterableMap<String, VRefundRequest>,
}

impl From<ContractDataV4> for ContractData {
    fn from(c: ContractDataV4) -> Self {
        let ContractDataV4 {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            pending_tx_limits,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            #[cfg(not(feature = "zcash"))]
            refund_requests,
        } = c;

        let new_config: Config = config.get().clone().unwrap().into();
        let ring_capacity = BlockAmountRing::capacity_for(&new_config);
        Self {
            config: LazyOption::new(StorageKey::Config, Some(new_config)),
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            pending_tx_limits,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            // Same versioned map, same storage prefix — old `VRefundRequest::V0`
            // entries are upgraded lazily to `Current` (adding `executed`) on read.
            #[cfg(not(feature = "zcash"))]
            refund_requests,
            // Zcash V4 had no refund_requests; initialize an empty map on upgrade.
            #[cfg(feature = "zcash")]
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
            block_bridge_amounts: BlockAmountRing::new(ring_capacity),
        }
    }
}

// Snapshot of the deployed (Bitcoin v0.8.x) `ContractData`, which was the enum's
// `Current` variant (discriminant 5) at deploy time. Structurally identical to the
// current `ContractData`: `config` is already the current `Config`, and
// `refund_requests` is the same versioned `VRefundRequest` map. It exists only to
// occupy discriminant 5; old `VRefundRequest::V0` entries upgrade lazily on read.
#[near(serializers = [borsh])]
pub struct ContractDataV5 {
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
    #[cfg(not(feature = "zcash"))]
    pub refund_requests: IterableMap<String, VRefundRequest>,
}

impl From<ContractDataV5> for ContractData {
    fn from(c: ContractDataV5) -> Self {
        let ContractDataV5 {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            pending_tx_limits,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            #[cfg(not(feature = "zcash"))]
            refund_requests,
        } = c;

        let ring_capacity = BlockAmountRing::capacity_for(
            config
                .get()
                .as_ref()
                .expect("ContractDataV5: config missing"),
        );

        Self {
            // `config` is already the current `Config` (with `unsafe_refund_timelock_sec`).
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            extra_msg_relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates,
            pending_tx_limits,
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
            // Same versioned map, same storage prefix — old `VRefundRequest::V0`
            // entries are upgraded lazily to `Current` (adding `executed`) on read.
            #[cfg(not(feature = "zcash"))]
            refund_requests,
            #[cfg(feature = "zcash")]
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
            block_bridge_amounts: BlockAmountRing::new(ring_capacity),
        }
    }
}
