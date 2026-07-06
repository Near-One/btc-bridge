use crate::{ext_contract, AccountId, PostAction, U128};

pub mod burn;
pub mod migration;
pub mod mint;

#[ext_contract(ext_nbtc)]
pub trait NBtc {
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
    fn ft_balance_of(&self, account_id: AccountId) -> U128;
    fn ft_total_supply(&self) -> U128;
}
