use crate::{
    env, ext_nbtc, is_promise_success, near, AccountId, Contract, ContractExt, Event, Gas, Promise,
    PromiseOrValue, U128,
};

pub const GAS_FOR_MIGRATION_BURN_CALL: Gas = Gas::from_tgas(80);
pub const GAS_FOR_MIGRATION_MINT_CALL: Gas = Gas::from_tgas(80);
pub const GAS_FOR_MIGRATION_RESOLVE_CALL_BACK: Gas = Gas::from_tgas(95);
pub const GAS_FOR_MIGRATION_MINT_CALL_BACK: Gas = Gas::from_tgas(185);

impl Contract {
    pub(crate) fn internal_migrate_to_new_token(
        &mut self,
        new_token: AccountId,
        accounts: Vec<AccountId>,
    ) -> Promise {
        let old_token = self.internal_config().nbtc_account_id.clone();
        ext_nbtc::ext(old_token)
            .with_static_gas(GAS_FOR_MIGRATION_BURN_CALL)
            .migration_burn(accounts)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_MIGRATION_MINT_CALL_BACK)
                    .migrate_to_new_token_mint(new_token),
            )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn migrate_to_new_token_mint(
        &mut self,
        new_token: AccountId,
        #[callback_unwrap] burned: Vec<(AccountId, U128)>,
    ) -> PromiseOrValue<()> {
        if burned.is_empty() {
            return PromiseOrValue::Value(());
        }

        ext_nbtc::ext(new_token.clone())
            .with_static_gas(GAS_FOR_MIGRATION_MINT_CALL)
            .migration_mint(burned.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_MIGRATION_RESOLVE_CALL_BACK)
                    .migrate_to_new_token_resolve(new_token, burned),
            )
            .into()
    }

    #[private]
    pub fn migrate_to_new_token_resolve(
        &mut self,
        new_token: AccountId,
        burned: Vec<(AccountId, U128)>,
    ) {
        let total_amount: u128 = burned.iter().map(|(_, amount)| amount.0).sum();
        let accounts = burned.len();

        if is_promise_success() {
            self.internal_mut_config().nbtc_account_id = new_token.clone();
            Event::TokenMigrated {
                new_token: &new_token,
                accounts,
                total_amount: U128(total_amount),
            }
            .emit();
        } else {
            let old_token = self.internal_config().nbtc_account_id.clone();
            ext_nbtc::ext(old_token)
                .with_static_gas(GAS_FOR_MIGRATION_MINT_CALL)
                .migration_mint(burned)
                .detach();
            Event::TokenMigrationRolledBack {
                new_token: &new_token,
                accounts,
                total_amount: U128(total_amount),
            }
            .emit();
        }
    }
}
