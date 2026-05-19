use crate::{env, near, Account, AccountId, Contract, ContractExt, NearToken, Promise};

use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};

#[near]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        let amount = env::attached_deposit();
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        let registration_only = registration_only.unwrap_or(false);
        if let Some(account) = self.accounts.get_mut(&account_id) {
            if registration_only && !amount.is_zero() {
                Promise::new(env::predecessor_account_id())
                    .transfer(amount)
                    .detach();
            } else {
                account.deposit += amount.as_yoctonear();
            }
        } else {
            let min_balance = self.storage_balance_bounds().min;
            if amount < min_balance {
                env::panic_str("The attached deposit is less than the mimimum storage balance");
            }

            let deposit = if registration_only {
                let refund = amount.as_yoctonear() - min_balance.as_yoctonear();
                if refund > 0 {
                    Promise::new(env::predecessor_account_id())
                        .transfer(NearToken::from_yoctonear(refund))
                        .detach();
                }
                min_balance.as_yoctonear()
            } else {
                amount.as_yoctonear()
            };

            let account = Account::new(&account_id, deposit);
            self.accounts.insert(account_id.clone(), account);
        }
        self.storage_balance_of(account_id).unwrap()
    }

    #[allow(unused_variables)]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        todo!()
    }

    #[allow(unused_variables)]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        todo!()
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: NearToken::from_millinear(100),
            max: None,
        }
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.accounts
            .get(&account_id)
            .map(|account| StorageBalance {
                total: NearToken::from_yoctonear(account.deposit),
                available: NearToken::from_yoctonear(account.deposit),
            })
    }
}
