use crate::{
    env, ext_nbtc, is_promise_success, json, near, require, serde_json, AccountId, Contract,
    ContractExt, Event, Gas, NearToken, Promise, PromiseOrValue, U128,
};

pub const GAS_FOR_FT_QUERY: Gas = Gas::from_tgas(3);
pub const GAS_FOR_MINT_ACTION: Gas = Gas::from_tgas(5);
pub const GAS_FOR_MIGRATION_MINT_CALL_BACK: Gas = Gas::from_tgas(30);
pub const GAS_FOR_MIGRATION_RESOLVE_CALL_BACK: Gas = Gas::from_tgas(10);

impl Contract {
    pub(crate) fn internal_migrate_to_new_token(
        &mut self,
        new_token: AccountId,
        accounts: Vec<AccountId>,
    ) -> Promise {
        let old_token = self.internal_config().nbtc_account_id.clone();
        require!(
            new_token != old_token,
            "New token must differ from the current token"
        );

        let mut queries = ext_nbtc::ext(old_token.clone())
            .with_static_gas(GAS_FOR_FT_QUERY)
            .ft_total_supply();
        for account in &accounts {
            queries = queries.and(
                ext_nbtc::ext(old_token.clone())
                    .with_static_gas(GAS_FOR_FT_QUERY)
                    .ft_balance_of(account.clone()),
            );
        }

        let callback_gas = Gas::from_gas(
            GAS_FOR_MIGRATION_MINT_CALL_BACK.as_gas()
                + GAS_FOR_MIGRATION_RESOLVE_CALL_BACK.as_gas()
                + GAS_FOR_MINT_ACTION.as_gas() * accounts.len() as u64,
        );
        queries.then(
            Self::ext(env::current_account_id())
                .with_static_gas(callback_gas)
                .migrate_to_new_token_mint(new_token, accounts),
        )
    }

    fn parse_ft_result(index: u64) -> u128 {
        let data = env::promise_result_checked(index, 64)
            .unwrap_or_else(|_| env::panic_str("Token query failed"));
        serde_json::from_slice::<U128>(&data)
            .unwrap_or_else(|_| env::panic_str("Failed to parse token query result"))
            .0
    }
}

#[near]
impl Contract {
    #[private]
    pub fn migrate_to_new_token_mint(
        &mut self,
        new_token: AccountId,
        accounts: Vec<AccountId>,
    ) -> PromiseOrValue<()> {
        require!(
            env::promise_results_count() == accounts.len() as u64 + 1,
            "Unexpected number of promise results"
        );

        let total_supply = Self::parse_ft_result(0);
        let mut sum: u128 = 0;
        let mut entries: Vec<(AccountId, u128)> = Vec::new();
        for (index, account) in accounts.into_iter().enumerate() {
            let balance = Self::parse_ft_result(index as u64 + 1);
            sum += balance;
            if balance > 0 {
                entries.push((account, balance));
            }
        }
        require!(
            sum == total_supply,
            "Sum of account balances does not match total supply"
        );

        let accounts_count = entries.len();
        let mut mint_batch = Promise::new(new_token.clone());
        for (account, amount) in entries {
            let args = serde_json::to_vec(&json!({
                "mint_account_id": account,
                "mint_amount": U128(amount),
                "protocol_fee": U128(0),
                "relayer_account_id": env::current_account_id(),
                "relayer_fee": U128(0),
                "post_actions": null,
            }))
            .unwrap_or_else(|_| env::panic_str("Failed to serialize mint args"));
            mint_batch = mint_batch.function_call(
                "mint".to_string(),
                args,
                NearToken::from_yoctonear(0),
                GAS_FOR_MINT_ACTION,
            );
        }
        mint_batch
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_MIGRATION_RESOLVE_CALL_BACK)
                    .migrate_to_new_token_resolve(new_token, accounts_count, U128(total_supply)),
            )
            .into()
    }

    #[private]
    pub fn migrate_to_new_token_resolve(
        &mut self,
        new_token: AccountId,
        accounts: usize,
        total_amount: U128,
    ) {
        require!(is_promise_success(), "Migration mint failed");
        self.internal_mut_config().nbtc_account_id = new_token.clone();
        Event::TokenMigrated {
            new_token: &new_token,
            accounts,
            total_amount,
        }
        .emit();
    }
}
