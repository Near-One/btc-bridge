use crate::*;
use near_contract_standards::fungible_token::core::ext_ft_core;

pub const GAS_FOR_TOKEN_TRANSFER: Gas = Gas::from_tgas(20);
pub const GAS_FOR_AFTER_TOKEN_TRANSFER: Gas = Gas::from_tgas(10);

impl Contract {
    pub fn internal_withdraw_protocol_fee(&self, amount: u128) -> Promise {
        ext_ft_core::ext(self.internal_config().nbtc_account_id.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_TOKEN_TRANSFER)
            .ft_transfer(env::predecessor_account_id(), amount.into(), None)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_AFTER_TOKEN_TRANSFER)
                    .withdraw_protocol_fee_callback(amount.into()),
            )
    }

    pub fn internal_transfer_nbtc(&self, account_id: &AccountId, amount: u128) -> Promise {
        ext_ft_core::ext(self.internal_config().nbtc_account_id.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FOR_TOKEN_TRANSFER)
            .ft_transfer(account_id.clone(), amount.into(), None)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_AFTER_TOKEN_TRANSFER)
                    .transfer_nbtc_callback(account_id.clone(), amount.into()),
            )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn withdraw_protocol_fee_callback(&mut self, amount: U128) -> bool {
        let promise_success = is_promise_success();
        let event = Event::WithdrawBridgeProtocolFee {
            amount,
            success: promise_success,
        };
        if !promise_success {
            self.data_mut().cur_available_protocol_fee += amount.0;
            self.data_mut().acc_claimed_protocol_fee -= amount.0;
        }
        event.emit();
        promise_success
    }

    #[private]
    pub fn transfer_nbtc_callback(&mut self, account_id: AccountId, amount: U128) -> bool {
        let promise_success = is_promise_success();
        let event = Event::TransferNbtc {
            account_id: &account_id,
            amount,
            success: promise_success,
        };
        if !promise_success {
            self.data_mut()
                .lost_found
                .entry(account_id.clone())
                .and_modify(|v| *v += amount.0)
                .or_insert(amount.0);
            Event::LostFoundNbtc {
                account_id: &account_id,
                amount,
            }
            .emit();
        }
        event.emit();
        promise_success
    }
}
