use crate::{ext_contract, AccountId, PostAction, U128};

pub mod burn;
pub mod migration;
pub mod mint;

#[ext_contract(ext_nbtc)]
pub trait NBtc {
    /// Standard mint used by the fee-charging deposit flow.
    ///
    /// NOTE: the nZec (Zcash) token implements only `safe_mint`, not `mint`, so
    /// on Zcash this call fails. Zcash deposits must therefore use the safe
    /// deposit flow (which mints via `safe_mint`).
    fn mint(
        &mut self,
        mint_account_id: AccountId,
        mint_amount: U128,
        protocol_fee: U128,
        relayer_account_id: AccountId,
        relayer_fee: U128,
        post_actions: Option<Vec<PostAction>>,
    );
    fn burn(
        &mut self,
        burn_account_id: AccountId,
        burn_amount: U128,
        relayer_account_id: AccountId,
        relayer_fee: U128,
    );
    fn safe_mint(&mut self, account_id: AccountId, amount: U128, msg: Option<String>);
    fn migration_burn(&mut self, accounts: Vec<AccountId>) -> Vec<(AccountId, U128)>;
    fn migration_mint(&mut self, entries: Vec<(AccountId, U128)>);
}
