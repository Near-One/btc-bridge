use crate::{near, u128_dec_format, AccountId, Contract};
use near_sdk::env;
use std::collections::HashSet;

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct OutstandingInfo {
    pub token_id: AccountId,
    #[serde(with = "u128_dec_format")]
    pub amount: u128,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct Account {
    pub account_id: AccountId,
    pub btc_pending_sign_ids: HashSet<String>,
    pub btc_pending_verify_list: HashSet<String>,
}

#[near(serializers = [borsh])]
pub enum VAccount {
    Current(Account),
}

impl From<VAccount> for Account {
    fn from(v: VAccount) -> Self {
        match v {
            VAccount::Current(c) => c,
        }
    }
}

impl From<&VAccount> for Account {
    fn from(v: &VAccount) -> Self {
        match v {
            VAccount::Current(c) => c.clone(),
        }
    }
}

impl<'a> From<&'a mut VAccount> for &'a mut Account {
    fn from(v: &'a mut VAccount) -> Self {
        match v {
            VAccount::Current(c) => c,
        }
    }
}

impl<'a> From<&'a VAccount> for &'a Account {
    fn from(v: &'a VAccount) -> Self {
        match v {
            VAccount::Current(c) => c,
        }
    }
}

impl From<Account> for VAccount {
    fn from(c: Account) -> Self {
        VAccount::Current(c)
    }
}

impl Account {
    pub fn new(account_id: &AccountId) -> Self {
        Self {
            account_id: account_id.clone(),
            btc_pending_sign_ids: HashSet::new(),
            btc_pending_verify_list: HashSet::new(),
        }
    }
}

impl Contract {
    pub fn check_account_exists(&self, account_id: &AccountId) -> bool {
        self.data().accounts.contains_key(account_id)
    }

    pub fn internal_get_account(&self, account_id: &AccountId) -> Option<&Account> {
        self.data().accounts.get(account_id).map(Into::into)
    }

    pub fn internal_unwrap_account(&self, account_id: &AccountId) -> &Account {
        self.data()
            .accounts
            .get(account_id)
            .map(|o| o.into())
            .unwrap_or_else(|| {
                env::panic_str(&format!("ERR_ACCOUNT_NOT_REGISTERED: {}", account_id))
            })
    }

    pub fn internal_unwrap_mut_account(&mut self, account_id: &AccountId) -> &mut Account {
        self.data_mut()
            .accounts
            .get_mut(account_id)
            .map(|o| o.into())
            .unwrap_or_else(|| {
                env::panic_str(&format!("ERR_ACCOUNT_NOT_REGISTERED: {}", account_id))
            })
    }

    pub fn internal_unwrap_or_create_mut_account(
        &mut self,
        account_id: &AccountId,
    ) -> &mut Account {
        self.data_mut()
            .accounts
            .entry(account_id.clone())
            .or_insert(Account::new(account_id).into())
            .into()
    }

    pub fn internal_set_account(&mut self, account_id: &AccountId, account: Account) {
        self.data_mut()
            .accounts
            .insert(account_id.clone(), account.into());
    }
}
