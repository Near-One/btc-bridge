use crate::{
    Contract, ContractExt, StorageKey, NO_DEPOSIT, OUTER_UPGRADE_GAS, WITHDRAW_RELAYER_ADDRESS,
};
use near_contract_standards::fungible_token::{metadata::FungibleTokenMetadata, FungibleToken};
use near_sdk::borsh::{self, BorshDeserialize};
use near_sdk::{
    collections::LazyOption, env, near, require, store::Lazy, AccountId, Promise, PublicKey,
};

const STATE_KEY: &[u8] = b"STATE";
const OWNABLE_KEY: &[u8] = b"__OWNER__";

#[near(serializers=[borsh])]
pub struct NearIntentsState {
    pub token: FungibleToken,
    pub metadata: Lazy<FungibleTokenMetadata>,
}

#[near]
impl Contract {
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// Attach a new full access to the current contract.
    pub fn attach_full_access_key(&mut self, public_key: PublicKey) -> Promise {
        self.assert_controller();
        Promise::new(env::current_account_id()).add_full_access_key(public_key)
    }

    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        env::state_read().unwrap_or_else(|| env::panic_str("ERR_FAILED_TO_READ_STATE"))
    }

    /// # Panics
    ///
    /// This function will panic if token is not in the expected state.
    #[private]
    #[init(ignore_state)]
    pub fn migrate_from_poa(
        controller: AccountId,
        bridge_id: AccountId,
        withdraw_relayer: &AccountId,
    ) -> Self {
        if !env::state_exists() {
            env::panic_str("Old state not found. Migration is not needed.")
        }

        let state = env::storage_read(STATE_KEY)
            .unwrap_or_else(|| env::panic_str("Failed to read state key."));

        if let Ok(state) = NearIntentsState::try_from_slice(&state) {
            require!(
                env::storage_remove(OWNABLE_KEY),
                "Wrong token version for migration: __OWNER__ key not found"
            );

            env::storage_write(
                WITHDRAW_RELAYER_ADDRESS,
                &borsh::to_vec(withdraw_relayer).unwrap(),
            );

            let new_state = Self {
                controller,
                bridge_id,
                token: state.token,
                metadata: LazyOption::new(StorageKey::Metadata, Some(state.metadata.get())),
            };

            new_state
        } else {
            env::panic_str("Old state not found. Migration is not needed.")
        }
    }

    pub fn upgrade_and_migrate(&self) {
        self.assert_controller();

        // Receive the code directly from the input to avoid the
        // GAS overhead of deserializing parameters
        let code = env::input().unwrap_or_else(|| env::panic_str("ERR_NO_INPUT"));
        // Deploy the contract code.
        let promise_id = env::promise_batch_create(&env::current_account_id());
        env::promise_batch_action_deploy_contract(promise_id, &code);
        // Call promise to migrate the state.
        // Batched together to fail upgrade if migration fails.
        env::promise_batch_action_function_call(
            promise_id,
            "migrate",
            b"",
            NO_DEPOSIT,
            env::prepaid_gas()
                .saturating_sub(env::used_gas())
                .saturating_sub(OUTER_UPGRADE_GAS),
        );
        env::promise_return(promise_id);
    }
}
