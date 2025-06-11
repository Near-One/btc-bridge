use crate::*;

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{log, require, serde_json};

#[near(serializers = [json])]
#[derive(Debug)]
pub enum TokenReceiverMsg {
    DepositToOthers { beneficiary_accouont_id: AccountId },
}

#[near]
impl FungibleTokenReceiver for Contract {
    #[allow(unused_variables)]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        if msg.is_empty() {
            require!(
                self.accounts.contains_key(&sender_id),
                format!("{} not register", sender_id)
            );
            log!(
                "dapp log(empty message): {} send {} {}",
                sender_id,
                amount.0,
                env::predecessor_account_id(),
            );
        } else {
            require!(
                self.accounts.contains_key(&sender_id),
                format!("{} not register", sender_id)
            );
            let _: TokenReceiverMsg =
                serde_json::from_str(&msg).expect("Can't parse TokenReceiverMsg");
            log!(
                "dapp log: {} send {} {} and msg {}",
                sender_id,
                amount.0,
                env::predecessor_account_id(),
                msg
            );
        }
        PromiseOrValue::Value(0.into())
    }
}
