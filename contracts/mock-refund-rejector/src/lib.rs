use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::json_types::U128;
use near_sdk::{env, near, serde_json, AccountId, Gas, NearToken, Promise, PromiseOrValue};

#[near(serializers = [json])]
pub struct RejectRefundMsg {
    pub bridge_id: AccountId,
    pub utxo_id: String,
}

#[derive(Default)]
#[near(contract_state)]
pub struct Contract {}

#[near]
impl FungibleTokenReceiver for Contract {
    #[allow(unused_variables)]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let msg: RejectRefundMsg = serde_json::from_str(&msg).expect("Can't parse RejectRefundMsg");
        PromiseOrValue::Promise(
            Promise::new(msg.bridge_id)
                .function_call(
                    "reject_refund".to_string(),
                    serde_json::to_vec(&serde_json::json!({ "utxo_storage_key": msg.utxo_id }))
                        .unwrap(),
                    NearToken::from_yoctonear(0),
                    Gas::from_tgas(10),
                )
                .then(
                    Self::ext(env::current_account_id())
                        .with_static_gas(Gas::from_tgas(5))
                        .return_unused(amount),
                ),
        )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn return_unused(&mut self, amount: U128) -> U128 {
        amount
    }
}
