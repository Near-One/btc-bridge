use near_sdk::{
    borsh::BorshSerialize, env, json_types::U128, near, store::IterableMap, AccountId,
    BorshStorageKey, NearToken, PanicOnDefault, Promise, PromiseOrValue,
};

mod account;
mod fungible_token;
mod storage;

use account::*;

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    pub accounts: IterableMap<AccountId, Account>,
}

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Accounts,
}

#[near]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self {
            accounts: IterableMap::new(StorageKey::Accounts),
        }
    }
}
