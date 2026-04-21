use crate::{
    env, near, u128_dec_format, AccessControllable, Account, AccountId, BTCPendingInfo, Config,
    Contract, ContractExt, HashMap, HashSet, NearToken, Pausable, Role, U128, UTXO,
};
#[cfg(not(feature = "zcash"))]
use crate::RefundRequest;

const REQUIRED_BALANCE_FOR_DEPOSIT: NearToken =
    NearToken::from_yoctonear(1_200_000_000_000_000_000_000);

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
    pub pending_tx_limits: HashMap<AccountId, u32>,
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
            super_admins: self.acl_get_super_admins(0, u64::from(u32::MAX)),
            daos: self.acl_get_grantees(Role::DAO.into(), 0, u64::from(u32::MAX)),
            operators: self.acl_get_grantees(Role::Operator.into(), 0, u64::from(u32::MAX)),
            pause_managers: self.acl_get_grantees(
                Role::PauseManager.into(),
                0,
                u64::from(u32::MAX),
            ),
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
            pending_tx_limits: root_state
                .pending_tx_limits
                .iter()
                .map(|(k, v)| (k.clone(), *v))
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

    pub fn get_account(&self, account_id: &AccountId) -> Option<Account> {
        self.data().accounts.get(account_id).map(Account::from)
    }

    pub fn list_accounts(
        &self,
        account_ids: Vec<AccountId>,
    ) -> HashMap<AccountId, Option<Account>> {
        account_ids
            .into_iter()
            .map(|key| {
                let account = self.get_account(&key);
                (key, account)
            })
            .collect()
    }

    pub fn get_accounts_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<AccountId, Account> {
        let len = usize::try_from(self.data().accounts.len())
            .unwrap_or_else(|_| env::panic_str("Too many accounts"));
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
        let len = usize::try_from(self.data().lost_found.len())
            .unwrap_or_else(|_| env::panic_str("Too many lost_found accounts"));
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
        let len = usize::try_from(self.data().utxos.len())
            .unwrap_or_else(|_| env::panic_str("Too many utxos"));
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
            .map(|key| (key.clone(), self.data().utxos.get(&key).map(Into::into)))
            .collect()
    }

    pub fn get_unavailable_utxos_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<String, UTXO> {
        let len = usize::try_from(self.data().unavailable_utxos.len())
            .unwrap_or_else(|_| env::panic_str("Too many unavailable_utxos"));
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
                    self.data().unavailable_utxos.get(&key).map(Into::into),
                )
            })
            .collect()
    }

    pub fn get_btc_pending_infos_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<String, BTCPendingInfo> {
        let len = usize::try_from(self.data().btc_pending_infos.len())
            .unwrap_or_else(|_| env::panic_str("Too many btc_pending_infos"));
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
        let len = usize::try_from(self.data().rbf_txs.len())
            .unwrap_or_else(|_| env::panic_str("Too many rbf_txs"));
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
        let len = usize::try_from(self.data().post_action_msg_templates.len())
            .unwrap_or_else(|_| env::panic_str("Too many post_action_msg_templates"));
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

    pub fn required_balance_for_safe_deposit(&self) -> NearToken {
        REQUIRED_BALANCE_FOR_DEPOSIT
    }

    #[cfg(not(feature = "zcash"))]
    pub fn get_refund_requests_paged(
        &self,
        from_index: Option<usize>,
        limit: Option<usize>,
    ) -> HashMap<String, RefundRequest> {
        let len = usize::try_from(self.data().refund_requests.len())
            .unwrap_or_else(|_| env::panic_str("Too many refund_requests"));
        let skip_n = from_index.unwrap_or(0);
        let take_n = limit.unwrap_or(len - skip_n);
        self.data()
            .refund_requests
            .iter()
            .skip(skip_n)
            .take(take_n)
            .map(|(k, v)| (k.clone(), v.into()))
            .collect()
    }

    #[cfg(not(feature = "zcash"))]
    pub fn required_balance_for_execute_refund(&self) -> NearToken {
        // execute_refund uses ~700 bytes of storage (BTCPendingInfo + Account + verified_deposit_utxo)
        // At 0.00001 NEAR/byte, that's ~0.007 NEAR. We use 0.01 NEAR as a safe margin.
        NearToken::from_millinear(10)
    }
}
