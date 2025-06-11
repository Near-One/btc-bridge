use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::receiver::ext_ft_receiver;
use near_contract_standards::fungible_token::resolver::ext_ft_resolver;
use near_contract_standards::fungible_token::{
    FungibleToken, FungibleTokenCore, FungibleTokenResolver,
};
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::LazyOption;
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::{
    assert_one_yocto, env, log, near, require, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, PromiseOrValue,
};

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    controller: AccountId,
    bridge_id: AccountId,
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg%20width%3D%2232%22%20height%3D%2232%22%20viewBox%3D%220%200%2032%2032%22%20fill%3D%22none%22%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%3E%3Cg%20clip-path%3D%22url(%23clip0_2351_779)%22%3E%3Cpath%20d%3D%22M16%2032C24.8366%2032%2032%2024.8366%2032%2016C32%207.16344%2024.8366%200%2016%200C7.16344%200%200%207.16344%200%2016C0%2024.8366%207.16344%2032%2016%2032Z%22%20fill%3D%22%2300E99F%22%2F%3E%3Cpath%20d%3D%22M16.0006%2028.2858C22.7858%2028.2858%2028.2863%2022.7853%2028.2863%2016.0001C28.2863%209.21486%2022.7858%203.71436%2016.0006%203.71436C9.21535%203.71436%203.71484%209.21486%203.71484%2016.0001C3.71484%2022.7853%209.21535%2028.2858%2016.0006%2028.2858Z%22%20stroke%3D%22black%22%2F%3E%3Cpath%20d%3D%22M27.1412%2016C27.1412%2022.1541%2022.1524%2027.1429%2015.9983%2027.1429C9.84427%2027.1429%204.85547%2022.1541%204.85547%2016C4.85547%209.84598%209.84427%204.85718%2015.9983%204.85718C22.1524%204.85718%2027.1412%209.84598%2027.1412%2016Z%22%20stroke%3D%22black%22%20stroke-width%3D%220.5%22%2F%3E%3Cpath%20fill-rule%3D%22evenodd%22%20clip-rule%3D%22evenodd%22%20d%3D%22M16.2167%2011.1743C15.9198%2011.1643%2015.6095%2011.1622%2015.2868%2011.1668V9.32056H13.8907V11.2217C13.1583%2011.2659%2012.3792%2011.3332%2011.5625%2011.4149V12.811H12.9586V18.8607H11.7952V20.4895H13.8893V22.5836H15.2854V20.4895H16.2161V22.5836H17.3795V20.4895C18.4654%2020.4119%2020.6836%2019.7915%2020.8698%2017.93C21.0559%2016.0686%2019.7064%2015.6032%2019.0083%2015.6032C19.5512%2015.3705%2020.544%2014.5328%2020.1717%2013.0436C19.9215%2012.043%2019.0072%2011.5204%2017.6128%2011.2984V9.32164H16.2167V11.1743ZM18.0737%2013.9723C18.0737%2012.8554%2016.2122%2012.7313%2015.2815%2012.8088V15.1356C16.2122%2015.2132%2018.0737%2015.0891%2018.0737%2013.9723ZM15.2826%2016.5322V18.8591C16.2133%2018.9366%2018.3075%2018.859%2018.3075%2017.6956C18.3075%2016.2994%2016.2133%2016.4547%2015.2826%2016.5322Z%22%20fill%3D%22black%22%2F%3E%3C%2Fg%3E%3Cdefs%3E%3CclipPath%20id%3D%22clip0_2351_779%22%3E%3Crect%20width%3D%2232%22%20height%3D%2232%22%20fill%3D%22white%22%2F%3E%3C%2FclipPath%3E%3C%2Fdefs%3E%3C%2Fsvg%3E";

const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas::from_tgas(5);
const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas::from_tgas(30);

const OUTER_UPGRADE_GAS: Gas = Gas::from_tgas(15);
const NO_DEPOSIT: NearToken = NearToken::from_yoctonear(0);

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    FungibleToken,
    Metadata,
}

#[near(serializers = [json])]
pub struct PostAction {
    pub receiver_id: AccountId,
    pub amount: U128,
    pub memo: Option<String>,
    pub msg: String,
    pub gas: Option<Gas>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(controller: AccountId, bridge_id: AccountId) -> Self {
        require!(!env::state_exists(), "Already initialized");
        Self {
            controller,
            bridge_id,
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(
                StorageKey::Metadata,
                Some(&FungibleTokenMetadata {
                    spec: FT_METADATA_SPEC.to_string(),
                    name: "Near WTC".to_string(),
                    symbol: "NBTC".to_string(),
                    icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                    reference: None,
                    reference_hash: None,
                    decimals: 8,
                }),
            ),
        }
    }

    #[payable]
    pub fn set_controller(&mut self, controller: AccountId) {
        assert_one_yocto();
        self.assert_controller();
        self.controller = controller;
    }

    pub fn mint(
        &mut self,
        mint_account_id: AccountId,
        mint_amount: U128,
        protocol_fee: U128,
        relayer_account_id: AccountId,
        relayer_fee: U128,
        post_actions: Option<Vec<PostAction>>,
    ) {
        self.assert_bridge();
        self.mint_inner(&mint_account_id, mint_amount);
        if protocol_fee.0 > 0 {
            self.mint_inner(&self.bridge_id.clone(), protocol_fee);
        }
        if relayer_fee.0 > 0 {
            self.mint_inner(&relayer_account_id, relayer_fee);
        }
        if let Some(post_actions) = post_actions {
            Self::ext(env::current_account_id()).handle_post_actions(mint_account_id, post_actions);
        }
    }

    pub fn burn(
        &mut self,
        burn_account_id: AccountId,
        burn_amount: U128,
        relayer_account_id: AccountId,
        relayer_fee: U128,
    ) {
        self.assert_bridge();
        self.token
            .internal_withdraw(&self.bridge_id, burn_amount.into());
        if relayer_fee.0 > 0 {
            if self.token.accounts.get(&relayer_account_id).is_none() {
                self.token.internal_register_account(&relayer_account_id);
            }
            self.token.internal_transfer(
                &self.bridge_id,
                &relayer_account_id,
                relayer_fee.into(),
                None,
            );
        }
        near_contract_standards::fungible_token::events::FtBurn {
            owner_id: &burn_account_id,
            amount: burn_amount,
            memo: None,
        }
        .emit();
    }
}

#[near]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for Contract {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let (used_amount, burned_amount) =
            self.token
                .internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        if burned_amount > 0 {
            log!("Account @{} burned {}", sender_id, burned_amount);
        }
        used_amount.into()
    }
}

#[near]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        self.token.storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        #[allow(unused_variables)]
        if let Some((account_id, balance)) = self.token.internal_storage_unregister(force) {
            log!("Closed @{} with {}", account_id, balance);
            true
        } else {
            false
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

#[near]
impl Contract {
    #[payable]
    pub fn set_metadata(
        &mut self,
        name: Option<String>,
        symbol: Option<String>,
        reference: Option<String>,
        reference_hash: Option<Base64VecU8>,
        decimals: Option<u8>,
        icon: Option<String>,
    ) {
        assert_one_yocto();
        self.assert_controller();

        let mut metadata = self.ft_metadata();
        if let Some(name) = name {
            metadata.name = name;
        }
        if let Some(symbol) = symbol {
            metadata.symbol = symbol;
        }
        if let Some(reference) = reference {
            metadata.reference = Some(reference);
        }
        if let Some(reference_hash) = reference_hash {
            metadata.reference_hash = Some(reference_hash);
        }
        if let Some(decimals) = decimals {
            // Decimals can't be changed if it's already set.
            if decimals != 0 {
                metadata.decimals = decimals;
            }
        }
        if let Some(icon) = icon {
            metadata.icon = Some(icon);
        }

        self.metadata.set(&metadata);
    }
}

#[near]
impl Contract {
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        env::state_read().unwrap_or_else(|| env::panic_str("ERR_FAILED_TO_READ_STATE"))
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

impl Contract {
    fn assert_bridge(&self) {
        require!(self.bridge_id == env::predecessor_account_id(), "Not Allow");
    }

    fn assert_controller(&self) {
        let caller = env::predecessor_account_id();
        require!(caller == self.controller, "ERR_MISSING_PERMISSION");
    }

    fn mint_inner(&mut self, account_id: &AccountId, amount: U128) {
        if self.token.accounts.get(account_id).is_none() {
            self.token.internal_register_account(account_id);
        }
        self.token.internal_deposit(account_id, amount.into());
        near_contract_standards::fungible_token::events::FtMint {
            owner_id: account_id,
            amount,
            memo: None,
        }
        .emit();
    }
}

#[near]
impl Contract {
    #[private]
    pub fn handle_post_actions(&mut self, sender_id: AccountId, post_actions: Vec<PostAction>) {
        for post_action in post_actions {
            let PostAction {
                receiver_id,
                amount,
                memo,
                msg,
                gas,
            } = post_action;
            if let Some(gas) = gas {
                Self::ext(env::current_account_id())
                    .with_static_gas(gas)
                    .handle_post_action(sender_id.clone(), receiver_id, amount, memo, msg)
            } else {
                Self::ext(env::current_account_id()).handle_post_action(
                    sender_id.clone(),
                    receiver_id,
                    amount,
                    memo,
                    msg,
                )
            };
        }
    }

    #[private]
    pub fn handle_post_action(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) {
        require!(
            env::prepaid_gas() > GAS_FOR_FT_TRANSFER_CALL,
            "More gas is required"
        );
        let amount = amount.into();
        self.token
            .internal_transfer(&sender_id, &receiver_id, amount, memo);
        let receiver_gas = env::prepaid_gas()
            .checked_sub(GAS_FOR_FT_TRANSFER_CALL)
            .unwrap_or_else(|| env::panic_str("Prepaid gas overflow"));
        // Initiating receiver's call and the callback
        ext_ft_receiver::ext(receiver_id.clone())
            .with_static_gas(receiver_gas)
            .ft_on_transfer(sender_id.clone(), amount.into(), msg)
            .then(
                ext_ft_resolver::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .ft_resolve_transfer(sender_id, receiver_id, amount.into()),
            );
    }
}
