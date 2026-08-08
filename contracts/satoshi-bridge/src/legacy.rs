use crate::VRefundRequest;
use crate::{
    near, AccountId, BlockAmountRing, Config, ContractData, HashSet, IterableMap, IterableSet,
    LazyOption, LookupSet, VAccount, VBTCPendingInfo, VUTXO,
};

#[near(serializers = [borsh])]
pub struct ContractDataV6 {
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
}

impl From<ContractDataV6> for ContractData {
    fn from(c: ContractDataV6) -> Self {
        let ContractDataV6 {
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
            refund_requests,
        } = c;

        let ring_capacity = BlockAmountRing::capacity_for(
            config
                .get()
                .as_ref()
                .expect("ContractDataV6: config missing"),
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
            refund_requests,
            block_bridge_amounts: BlockAmountRing::new(ring_capacity),
        }
    }
}
