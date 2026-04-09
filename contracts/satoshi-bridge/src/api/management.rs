use crate::{
    assert_one_yocto, env, near, require, AccessControllable, Account, AccountId, BridgeFee,
    Contract, ContractExt, HashSet, Promise, Role, U128, U64,
};

use near_plugins::access_control_any;

#[near]
impl Contract {
    /// Withdraw a specified amount of protocol fee to the owner’s account.
    ///
    /// # Arguments
    ///
    /// * `amount` - Specify the amount to withdraw; if not specified, it will be the full amount.
    ///
    /// # Returns
    ///
    /// bool - Whether the Withdraw was successful.
    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn withdraw_protocol_fee(&mut self, amount: Option<U128>) -> Promise {
        assert_one_yocto();
        let total_protocol_fee = self.data().cur_available_protocol_fee;
        let amount = amount.map_or(total_protocol_fee, |v| v.0);
        require!(amount > 0 && amount <= total_protocol_fee, "Invalid amount");
        self.data_mut().cur_available_protocol_fee -= amount;
        self.data_mut().acc_claimed_protocol_fee += amount;
        self.internal_withdraw_protocol_fee(amount)
    }
}

#[near]
impl Contract {
    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn add_super_admin(&mut self, account_id: AccountId) {
        assert_one_yocto();
        let is_success = self.acl_add_super_admin(account_id.clone()).unwrap();
        require!(is_success, "acl_add_super_admin failed");
        let is_success = self
            .acl_grant_role(Role::DAO.into(), account_id.clone())
            .unwrap();
        require!(is_success, "acl_grant_role DAO failed");
        let is_success = self
            .acl_grant_role(Role::PauseManager.into(), account_id.clone())
            .unwrap();
        require!(is_success, "acl_grant_role PauseManager failed");
        if !self.check_account_exists(&account_id) {
            self.internal_set_account(&account_id, Account::new(&account_id));
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_super_admin(&mut self, account_id: AccountId) {
        assert_one_yocto();
        require!(
            env::predecessor_account_id() != account_id,
            "cannot remove oneself"
        );
        let is_success = self
            .acl_revoke_role(Role::DAO.into(), account_id.clone())
            .unwrap();
        require!(is_success, "acl_revoke_role DAO failed");
        let is_success = self
            .acl_revoke_role(Role::PauseManager.into(), account_id.clone())
            .unwrap();
        require!(is_success, "acl_revoke_role PauseManager failed");
        let is_success = self.acl_revoke_super_admin(account_id.clone()).unwrap();
        require!(is_success, "acl_revoke_super_admin failed");
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn extend_operators(&mut self, operators: Vec<AccountId>) {
        assert_one_yocto();
        for operator in operators {
            let is_success = self
                .acl_grant_role(Role::Operator.into(), operator.clone())
                .unwrap();
            require!(is_success, format!("Already exist operator: {}", operator));
            if !self.check_account_exists(&operator) {
                self.internal_set_account(&operator, Account::new(&operator));
            }
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_operators(&mut self, operators: Vec<AccountId>) {
        assert_one_yocto();
        for operator in operators {
            let is_success = self
                .acl_revoke_role(Role::Operator.into(), operator.clone())
                .unwrap();
            require!(is_success, format!("Invalid operator: {}", operator));
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn extend_relayer_white_list(&mut self, relayer_ids: Vec<AccountId>) {
        assert_one_yocto();
        for relayer_id in relayer_ids {
            let is_success = self
                .data_mut()
                .relayer_white_list
                .insert(relayer_id.clone());
            require!(
                is_success,
                format!("Already exist relayer_id: {}", relayer_id)
            );
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_relayer_white_list(&mut self, relayer_ids: Vec<AccountId>) {
        assert_one_yocto();
        for relayer_id in relayer_ids {
            let is_success = self.data_mut().relayer_white_list.remove(&relayer_id);
            require!(is_success, format!("Invalid relayer_id: {}", relayer_id));
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn extend_extra_msg_relayer_white_list(&mut self, relayer_ids: Vec<AccountId>) {
        assert_one_yocto();
        for relayer_id in relayer_ids {
            let is_success = self
                .data_mut()
                .extra_msg_relayer_white_list
                .insert(relayer_id.clone());
            require!(
                is_success,
                format!("Already exist relayer_id: {}", relayer_id)
            );
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_extra_msg_relayer_white_list(&mut self, relayer_ids: Vec<AccountId>) {
        assert_one_yocto();
        for relayer_id in relayer_ids {
            let is_success = self
                .data_mut()
                .extra_msg_relayer_white_list
                .remove(&relayer_id);
            require!(is_success, format!("Invalid relayer_id: {}", relayer_id));
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn extend_post_action_receiver_id_white_list(&mut self, receiver_ids: Vec<AccountId>) {
        assert_one_yocto();
        for receiver_id in receiver_ids {
            let is_success = self
                .data_mut()
                .post_action_receiver_id_white_list
                .insert(receiver_id.clone());
            require!(
                is_success,
                format!("Already exist receiver_id: {}", receiver_id)
            );
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_post_action_receiver_id_white_list(&mut self, receiver_ids: Vec<AccountId>) {
        assert_one_yocto();
        for receiver_id in receiver_ids {
            let is_success = self
                .data_mut()
                .post_action_receiver_id_white_list
                .remove(&receiver_id);
            require!(is_success, format!("Invalid receiver_id: {}", receiver_id));
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn extend_multi_txs_white_list(&mut self, account_ids: Vec<AccountId>) {
        assert_one_yocto();
        for account_id in account_ids {
            let is_success = self
                .data_mut()
                .multi_txs_white_list
                .insert(account_id.clone());
            require!(
                is_success,
                format!("Already exist account_id: {}", account_id)
            );
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_multi_txs_white_list(&mut self, account_ids: Vec<AccountId>) {
        assert_one_yocto();
        for account_id in account_ids {
            let is_success = self.data_mut().multi_txs_white_list.remove(&account_id);
            require!(is_success, format!("Invalid account_id: {}", account_id));
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn extend_post_action_msg_templates(
        &mut self,
        contract_id: AccountId,
        templates: HashSet<String>,
    ) {
        assert_one_yocto();
        require!(!templates.is_empty(), "empty templates.");
        if let Some(msg_templates) = self
            .data_mut()
            .post_action_msg_templates
            .get_mut(&contract_id)
        {
            for template in templates {
                let is_success = msg_templates.insert(template.clone());
                require!(is_success, format!("{:?} is exist.", template));
            }
        } else {
            self.data_mut()
                .post_action_msg_templates
                .insert(contract_id, templates);
        }
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_post_action_msg_templates(
        &mut self,
        contract_id: AccountId,
        templates: Option<HashSet<String>>,
    ) {
        assert_one_yocto();
        if let Some(mut msg_templates) = self
            .data_mut()
            .post_action_msg_templates
            .remove(&contract_id)
        {
            if let Some(templates) = templates {
                require!(!templates.is_empty(), "empty templates.");
                for template in templates {
                    let is_success = msg_templates.remove(&template);
                    require!(is_success, format!("{:?} is not exist.", template));
                }
                if !msg_templates.is_empty() {
                    self.data_mut()
                        .post_action_msg_templates
                        .insert(contract_id, msg_templates);
                }
            }
        } else {
            env::panic_str("Invalid contract_id.");
        }
    }
}

#[near]
impl Contract {
    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn sync_chain_signatures_root_public_key(&mut self) -> Promise {
        assert_one_yocto();
        require!(
            self.internal_config()
                .chain_signatures_root_public_key
                .is_none(),
            "Already sync"
        );
        self.sync_chain_signatures_root_public_key_promise()
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_btc_light_client_account_id(&mut self, btc_light_client_account_id: AccountId) {
        assert_one_yocto();
        self.internal_mut_config().btc_light_client_account_id = btc_light_client_account_id;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_nbtc_account_id(&mut self, nbtc_account_id: AccountId) {
        assert_one_yocto();
        self.internal_mut_config().nbtc_account_id = nbtc_account_id;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_confirmations_strategy(&mut self, range_upper_bound: U128, confirmations: u8) {
        assert_one_yocto();
        require!(
            (2..=10).contains(&confirmations),
            "The number of confirmations must be between 2 and 10, including both 2 and 10."
        );
        self.internal_mut_config()
            .confirmations_strategy
            .insert(range_upper_bound.0.to_string(), confirmations);
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn remove_confirmations_strategy(&mut self, range_upper_bound: U128) {
        assert_one_yocto();
        let is_success = self
            .internal_mut_config()
            .confirmations_strategy
            .remove(&range_upper_bound.0.to_string())
            .is_some();
        require!(is_success, "Invalid range_upper_bound");
        require!(
            !self.internal_config().confirmations_strategy.is_empty(),
            "confirmations_strategy must not be empty"
        );
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_confirmations_delta(&mut self, confirmations_delta: u8) {
        assert_one_yocto();
        self.internal_mut_config().confirmations_delta = confirmations_delta;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_extra_msg_confirmations_delta(&mut self, extra_msg_confirmations_delta: u8) {
        assert_one_yocto();
        self.internal_mut_config().extra_msg_confirmations_delta = extra_msg_confirmations_delta;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_deposit_bridge_fee(&mut self, deposit_bridge_fee: BridgeFee) {
        assert_one_yocto();
        deposit_bridge_fee.assert_valid();
        self.internal_mut_config().deposit_bridge_fee = deposit_bridge_fee;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_withdraw_bridge_fee(&mut self, withdraw_bridge_fee: BridgeFee) {
        assert_one_yocto();
        withdraw_bridge_fee.assert_valid();
        self.internal_mut_config().withdraw_bridge_fee = withdraw_bridge_fee;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_min_deposit_amount(&mut self, min_deposit_amount: U128) {
        assert_one_yocto();
        self.internal_mut_config().min_deposit_amount = min_deposit_amount.into();
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_min_withdraw_amount(&mut self, min_withdraw_amount: U128) {
        assert_one_yocto();
        self.internal_mut_config().min_withdraw_amount = min_withdraw_amount.into();
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_change_amount_range(&mut self, min_change_amount: U128, max_change_amount: U128) {
        assert_one_yocto();
        require!(
            min_change_amount.0 < max_change_amount.0,
            "min_change_amount must be less than max_change_amount"
        );
        let config = self.internal_mut_config();
        config.min_change_amount = min_change_amount.into();
        config.max_change_amount = max_change_amount.into();
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_btc_gas_fee_valid_range(&mut self, min_btc_gas_fee: U128, max_btc_gas_fee: U128) {
        assert_one_yocto();
        require!(
            min_btc_gas_fee.0 < max_btc_gas_fee.0,
            "min_btc_gas_fee must be less than max_btc_gas_fee"
        );
        let config = self.internal_mut_config();
        config.min_btc_gas_fee = min_btc_gas_fee.into();
        config.max_btc_gas_fee = max_btc_gas_fee.into();
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_max_withdrawal_input_number(&mut self, max_withdrawal_input_number: u8) {
        assert_one_yocto();
        self.internal_mut_config().max_withdrawal_input_number = max_withdrawal_input_number;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_max_change_number(&mut self, max_change_number: u8) {
        assert_one_yocto();
        self.internal_mut_config().max_change_number = max_change_number;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_max_active_utxo_management_input_number(
        &mut self,
        max_active_utxo_management_input_number: u8,
    ) {
        assert_one_yocto();
        self.internal_mut_config()
            .max_active_utxo_management_input_number = max_active_utxo_management_input_number;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_max_active_utxo_management_output_number(
        &mut self,
        max_active_utxo_management_output_number: u8,
    ) {
        assert_one_yocto();
        self.internal_mut_config()
            .max_active_utxo_management_output_number = max_active_utxo_management_output_number;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_active_management_limit(
        &mut self,
        active_management_lower_limit: u32,
        active_management_upper_limit: u32,
    ) {
        assert_one_yocto();
        require!(
            active_management_lower_limit < active_management_upper_limit,
            "active_management_lower_limit must be less than active_management_upper_limit"
        );
        let config = self.internal_mut_config();
        config.active_management_lower_limit = active_management_lower_limit;
        config.active_management_upper_limit = active_management_upper_limit;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_passive_management_limit(
        &mut self,
        passive_management_lower_limit: u32,
        passive_management_upper_limit: u32,
    ) {
        assert_one_yocto();
        require!(
            passive_management_lower_limit < passive_management_upper_limit,
            "passive_management_lower_limit must be less than passive_management_upper_limit"
        );
        let config = self.internal_mut_config();
        config.passive_management_lower_limit = passive_management_lower_limit;
        config.passive_management_upper_limit = passive_management_upper_limit;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_rbf_num_limit(&mut self, rbf_num_limit: u8) {
        assert_one_yocto();
        self.internal_mut_config().rbf_num_limit = rbf_num_limit;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_max_btc_tx_pending_sec(&mut self, max_btc_tx_pending_sec: u32) {
        assert_one_yocto();
        self.internal_mut_config().max_btc_tx_pending_sec = max_btc_tx_pending_sec;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_max_pending_sign_txs(&mut self, max_pending_sign_txs: u32) {
        assert_one_yocto();
        require!(max_pending_sign_txs >= 1, "Invalid max_pending_sign_txs");
        self.internal_mut_config().max_pending_sign_txs = max_pending_sign_txs;
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_unhealthy_utxo_amount(&mut self, unhealthy_utxo_amount: U64) {
        assert_one_yocto();
        require!(
            u128::from(unhealthy_utxo_amount.0) > self.internal_config().min_change_amount,
            "Invalid unhealthy_utxo_amount"
        );
        self.internal_mut_config().unhealthy_utxo_amount = unhealthy_utxo_amount.0;
    }
}
