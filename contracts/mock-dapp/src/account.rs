use crate::*;

#[near(serializers = [borsh, json])]
pub struct Account {
    pub account_id: AccountId,
    pub deposit: u128,
}

impl Account {
    pub fn new(account_id: &AccountId, deposit: u128) -> Self {
        Self {
            account_id: account_id.clone(),
            deposit,
        }
    }
}
