use crate::{
    assert_one_yocto, env, get_deposit_path, near, require, AccessControllable, Account, AccountId,
    ConfigUpdate, Contract, ContractExt, DepositMsg, Event, HashSet, Promise, Role, U128,
};

use near_plugins::access_control_any;
use near_sdk::json_types::Base64VecU8;

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
        let is_success = self
            .acl_grant_role(Role::UnpauseManager.into(), account_id.clone())
            .unwrap();
        require!(is_success, "acl_grant_role UnpauseManager failed");
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
        // Accounts created before UnpauseManager existed may not hold this role; tolerate that.
        self.acl_revoke_role(Role::UnpauseManager.into(), account_id.clone());
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
    pub fn set_pending_tx_limit(&mut self, account_id: AccountId, max_pending: Option<u32>) {
        assert_one_yocto();
        if let Some(max_pending) = max_pending {
            require!(max_pending >= 1, "Invalid max_pending value");
            self.data_mut()
                .pending_tx_limits
                .insert(account_id, max_pending);
        } else {
            let prev = self.data_mut().pending_tx_limits.remove(&account_id);
            require!(
                prev.is_some(),
                format!("Invalid account_id: {}", account_id)
            );
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
    pub fn update_config(&mut self, update: ConfigUpdate) {
        assert_one_yocto();
        update.apply(self.internal_mut_config());
    }

    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn set_confirmations_strategy(&mut self, range_upper_bound: U128, confirmations: u8) {
        assert_one_yocto();

        let config = self.internal_mut_config();
        config
            .confirmations_strategy
            .insert(range_upper_bound.0.to_string(), confirmations);

        config.assert_valid()
    }

    /// Register the UTXO of a deposit whose `mint_callback` failed after nBTC was already minted.
    #[payable]
    #[access_control_any(roles(Role::DAO))]
    pub fn complete_failed_deposit_mint(
        &mut self,
        deposit_msg: DepositMsg,
        tx_bytes: Base64VecU8,
        vout: usize,
    ) -> String {
        assert_one_yocto();
        let pending_utxo_info = self.internal_build_deposit_utxo_info(
            get_deposit_path(&deposit_msg),
            &tx_bytes.0,
            vout,
        );
        let utxo_storage_key = pending_utxo_info.utxo_storage_key;

        require!(
            self.data()
                .verified_deposit_utxo
                .contains(&utxo_storage_key),
            "Deposit is not verified"
        );
        require!(
            !self.data().utxos.contains_key(&utxo_storage_key),
            "UTXO already registered"
        );
        require!(
            !self
                .data()
                .unavailable_utxos
                .contains_key(&utxo_storage_key),
            "UTXO is unavailable"
        );
        // A refund spends the very same output, so the UTXO must not be claimed by one.
        require!(
            !self.data().refund_requests.contains_key(&utxo_storage_key),
            "UTXO is claimed by a refund"
        );

        self.internal_set_utxo(&utxo_storage_key, pending_utxo_info.utxo);
        self.internal_remove_utxo_in_progress(&utxo_storage_key);

        utxo_storage_key
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
}
