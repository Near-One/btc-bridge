use crate::*;

#[near(serializers = [json])]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct Metadata {
    pub super_admins: Vec<AccountId>,
    pub daos: Vec<AccountId>,
    pub operators: Vec<AccountId>,
    pub pause_managers: Vec<AccountId>,
    pub pa_all_paused: Option<HashSet<String>>,
    pub relayer_white_list: Vec<AccountId>,
    pub extra_msg_relayer_white_list: Vec<AccountId>,
    pub post_action_receiver_id_white_list: Vec<AccountId>,
    #[serde(with = "u128_dec_format")]
    pub acc_collected_protocol_fee: u128,
    #[serde(with = "u128_dec_format")]
    pub cur_available_protocol_fee: u128,
    #[serde(with = "u128_dec_format")]
    pub acc_claimed_protocol_fee: u128,
    #[serde(with = "u128_dec_format")]
    pub cur_reserved_protocol_fee: u128,
    #[serde(with = "u128_dec_format")]
    pub acc_protocol_fee_for_gas: u128,
    pub current_accounts_num: u32,
    pub current_utxos_num: u32,
    pub version: String,
}

#[near]
impl Contract {
    pub fn get_metadata(&self) -> Metadata {
        let root_state = self.data();
        Metadata {
            super_admins: self.acl_get_super_admins(0, usize::MAX as u64),
            daos: self.acl_get_grantees(Role::DAO.into(), 0, usize::MAX as u64),
            operators: self.acl_get_grantees(Role::Operator.into(), 0, usize::MAX as u64),
            pause_managers: self.acl_get_grantees(Role::PauseManager.into(), 0, usize::MAX as u64),
            pa_all_paused: self.pa_all_paused(),
            relayer_white_list: root_state.relayer_white_list.iter().cloned().collect(),
            extra_msg_relayer_white_list: root_state
                .extra_msg_relayer_white_list
                .iter()
                .cloned()
                .collect(),
            post_action_receiver_id_white_list: root_state
                .post_action_receiver_id_white_list
                .iter()
                .cloned()
                .collect(),
            acc_collected_protocol_fee: root_state.acc_collected_protocol_fee,
            cur_available_protocol_fee: root_state.cur_available_protocol_fee,
            acc_claimed_protocol_fee: root_state.acc_claimed_protocol_fee,
            cur_reserved_protocol_fee: root_state.cur_reserved_protocol_fee,
            acc_protocol_fee_for_gas: root_state.acc_protocol_fee_for_gas,
            current_accounts_num: root_state.accounts.len(),
            current_utxos_num: root_state.utxos.len(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn get_config(&self) -> Config {
        self.internal_config().clone()
    }

    pub fn get_account(&self, account_id: AccountId) -> Option<Account> {
        self.internal_get_account(&account_id).cloned()
    }

    pub fn list_accounts(
        &self,
        account_ids: Vec<AccountId>,
    ) -> HashMap<AccountId, Option<Account>> {
        account_ids
            .into_iter()
            .map(|key| (key.clone(), self.internal_get_account(&key).cloned()))
            .collect()
    }

    pub fn get_accounts_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<AccountId, Account> {
        let len = self.data().accounts.len() as usize;
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .accounts
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), v.into()))
            .collect()
    }

    pub fn get_lost_found_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<AccountId, U128> {
        let len = self.data().lost_found.len() as usize;
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .lost_found
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), U128(*v)))
            .collect()
    }

    pub fn list_lost_found(&self, account_ids: Vec<AccountId>) -> HashMap<AccountId, Option<U128>> {
        account_ids
            .into_iter()
            .map(|key| {
                (
                    key.clone(),
                    self.data().lost_found.get(&key).map(|v| U128(*v)),
                )
            })
            .collect()
    }

    pub fn get_utxos_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<String, UTXO> {
        let len = self.data().utxos.len() as usize;
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .utxos
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), v.into()))
            .collect()
    }

    pub fn list_utxos(&self, utxo_storage_keys: Vec<String>) -> HashMap<String, Option<UTXO>> {
        utxo_storage_keys
            .into_iter()
            .map(|key| (key.clone(), self.data().utxos.get(&key).map(|v| v.into())))
            .collect()
    }

    pub fn get_unavailable_utxos_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<String, UTXO> {
        let len = self.data().unavailable_utxos.len() as usize;
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .unavailable_utxos
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), v.into()))
            .collect()
    }

    pub fn list_unavailable_utxos(
        &self,
        utxo_storage_keys: Vec<String>,
    ) -> HashMap<String, Option<UTXO>> {
        utxo_storage_keys
            .into_iter()
            .map(|key| {
                (
                    key.clone(),
                    self.data().unavailable_utxos.get(&key).map(|v| v.into()),
                )
            })
            .collect()
    }

    pub fn get_btc_pending_infos_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<String, BTCPendingInfo> {
        let len = self.data().btc_pending_infos.len() as usize;
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .btc_pending_infos
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), v.into()))
            .collect()
    }

    pub fn list_btc_pending_infos(
        &self,
        btc_pending_ids: Vec<String>,
    ) -> HashMap<String, Option<BTCPendingInfo>> {
        btc_pending_ids
            .into_iter()
            .map(|key| (key.clone(), self.internal_view_btc_pending_info(&key)))
            .collect()
    }

    pub fn list_tx_rbf_ids(
        &self,
        btc_pending_verify_ids: Vec<String>,
    ) -> HashMap<String, Option<HashSet<String>>> {
        btc_pending_verify_ids
            .into_iter()
            .map(|key| (key.clone(), self.data().rbf_txs.get(&key).cloned()))
            .collect()
    }

    pub fn get_rbf_txs_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<String, HashSet<String>> {
        let len = self.data().rbf_txs.len() as usize;
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .rbf_txs
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    pub fn list_post_action_msg_templates(
        &self,
        contract_ids: Vec<AccountId>,
    ) -> HashMap<AccountId, Option<HashSet<String>>> {
        contract_ids
            .into_iter()
            .map(|key| {
                (
                    key.clone(),
                    self.data().post_action_msg_templates.get(&key).cloned(),
                )
            })
            .collect()
    }

    pub fn get_post_action_msg_templates_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<AccountId, HashSet<String>> {
        let len = self.data().post_action_msg_templates.len() as usize;
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .post_action_msg_templates
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}
