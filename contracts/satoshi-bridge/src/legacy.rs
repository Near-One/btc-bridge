use crate::*;

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

        Self {
            config,
            accounts,
            utxos,
            unavailable_utxos,
            verified_deposit_utxo,
            btc_pending_infos,
            rbf_txs,
            relayer_white_list,
            post_action_receiver_id_white_list,
            post_action_msg_templates: IterableMap::new(StorageKey::PostActionMsgTemplates),
            lost_found,
            acc_collected_protocol_fee,
            cur_available_protocol_fee,
            acc_claimed_protocol_fee,
            cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas,
        }
    }
}
