#[cfg(not(feature = "zcash"))]
use crate::VRefundRequest;
use crate::{
    env, near, u128_dec_format, AccountId, BTCPendingInfo, BridgeFee, Config, ContractData,
    HashMap, HashSet, IterableMap, IterableSet, LazyOption, LookupSet, OriginalState,
    PendingInfoStage, PendingInfoState, PublicKey, RbfState, SignatureResponse, StorageKey,
    VAccount, VBTCPendingInfo, U128, VUTXO,
};

fn migrate_btc_pending_infos_to_current(
    btc_pending_infos: &mut IterableMap<String, VBTCPendingInfo>,
) {
    let keys: Vec<String> = btc_pending_infos.keys().cloned().collect();
    for key in keys {
        if let Some(value) = btc_pending_infos.get(&key) {
            let current: BTCPendingInfo = value.into();
            btc_pending_infos.insert(key, VBTCPendingInfo::Current(current));
        }
    }
}

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
            mut btc_pending_infos,
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

        migrate_btc_pending_infos_to_current(&mut btc_pending_infos);

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
            #[cfg(not(feature = "zcash"))]
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
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
            mut btc_pending_infos,
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

        migrate_btc_pending_infos_to_current(&mut btc_pending_infos);

        let config_v0 = config.get().clone().unwrap();
        Self {
            config: LazyOption::new(StorageKey::Config, Some(config_v0.into())),
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
            #[cfg(not(feature = "zcash"))]
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
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
            mut btc_pending_infos,
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

        migrate_btc_pending_infos_to_current(&mut btc_pending_infos);

        Self {
            config: LazyOption::new(
                StorageKey::Config,
                Some(config.get().clone().unwrap().into()),
            ),
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
            #[cfg(not(feature = "zcash"))]
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
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
            mut btc_pending_infos,
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

        migrate_btc_pending_infos_to_current(&mut btc_pending_infos);

        Self {
            config: LazyOption::new(
                StorageKey::Config,
                Some(config.get().clone().unwrap().into()),
            ),
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
            #[cfg(not(feature = "zcash"))]
            refund_requests: IterableMap::new(StorageKey::RefundRequests),
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
            mut btc_pending_infos,
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

        migrate_btc_pending_infos_to_current(&mut btc_pending_infos);

        Self {
            config: LazyOption::new(
                StorageKey::Config,
                Some(config.get().clone().unwrap().into()),
            ),
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
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct OriginalStateV0 {
    pub stage: PendingInfoStage,
    #[serde(with = "u128_dec_format")]
    pub max_gas_fee: u128,
    pub last_rbf_time_sec: Option<u32>,
    pub cancel_rbf_reserved: Option<U128>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub enum PendingInfoStateV0 {
    WithdrawOriginal(OriginalStateV0),
    WithdrawUserRbf(RbfState),
    WithdrawCancelRbf(RbfState),
    ActiveUtxoManagementOriginal(OriginalStateV0),
    ActiveUtxoManagementRbf(RbfState),
    ActiveUtxoManagementCancelRbf(RbfState),
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct BTCPendingInfoV0 {
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
    pub state: PendingInfoStateV0,
}

impl From<OriginalStateV0> for OriginalState {
    fn from(c: OriginalStateV0) -> Self {
        Self {
            stage: c.stage,
            max_gas_fee: c.max_gas_fee,
            last_rbf_time_sec: c.last_rbf_time_sec,
            cancel_rbf_reserved: c.cancel_rbf_reserved,
            subsidize_amount: 0,
        }
    }
}

impl From<PendingInfoStateV0> for PendingInfoState {
    fn from(c: PendingInfoStateV0) -> Self {
        match c {
            PendingInfoStateV0::WithdrawOriginal(x) => PendingInfoState::WithdrawOriginal(x.into()),
            PendingInfoStateV0::WithdrawUserRbf(x) => PendingInfoState::WithdrawUserRbf(x),
            PendingInfoStateV0::WithdrawCancelRbf(x) => PendingInfoState::WithdrawCancelRbf(x),
            PendingInfoStateV0::ActiveUtxoManagementOriginal(x) => {
                PendingInfoState::ActiveUtxoManagementOriginal(x.into())
            }
            PendingInfoStateV0::ActiveUtxoManagementRbf(x) => {
                PendingInfoState::ActiveUtxoManagementRbf(x)
            }
            PendingInfoStateV0::ActiveUtxoManagementCancelRbf(x) => {
                PendingInfoState::ActiveUtxoManagementCancelRbf(x)
            }
        }
    }
}

impl From<BTCPendingInfoV0> for BTCPendingInfo {
    fn from(c: BTCPendingInfoV0) -> Self {
        Self {
            account_id: c.account_id,
            btc_pending_id: c.btc_pending_id,
            transfer_amount: c.transfer_amount,
            actual_received_amount: c.actual_received_amount,
            withdraw_fee: c.withdraw_fee,
            gas_fee: c.gas_fee,
            burn_amount: c.burn_amount,
            psbt_hex: c.psbt_hex,
            vutxos: c.vutxos,
            signatures: c.signatures,
            tx_bytes_with_sign: c.tx_bytes_with_sign,
            create_time_sec: c.create_time_sec,
            last_sign_time_sec: c.last_sign_time_sec,
            state: c.state.into(),
        }
    }
}
