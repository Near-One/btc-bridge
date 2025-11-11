#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]
use std::collections::HashSet;
use std::{collections::HashMap, process::exit};

use bitcoin::hex::Case;
use bitcoin::{OutPoint, TxOut};
use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_sdk::{
    base64::{self, Engine},
    json_types::U128,
    serde_json::{json, Value},
    AccountId, Gas, NearToken,
};
use near_workspaces::{
    compile_project,
    network::Sandbox,
    result::ExecutionFinalResult,
    Account, Contract, Result, Worker,
};
use satoshi_bridge::{
    btc_light_client::deposit, BTCPendingInfo, DepositMsg, Metadata, TokenReceiverMessage, UTXO,
};

use crate::setup::{PRICE_ORICE_NEAR_PRICE_ID, PYTH_ORICE_NEAR_PRICE_ID};

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg%20width%3D%2232%22%20height%3D%2232%22%20viewBox%3D%220%200%2032%2032%22%20fill%3D%22none%22%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%3E%3Cg%20clip-path%3D%22url(%23clip0_2351_779)%22%3E%3Cpath%20d%3D%22M16%2032C24.8366%2032%2032%2024.8366%2032%2016C32%207.16344%2024.8366%200%2016%200C7.16344%200%200%207.16344%200%2016C0%2024.8366%207.16344%2032%2016%2032Z%22%20fill%3D%22%2300E99F%22%2F%3E%3Cpath%20d%3D%22M16.0006%2028.2858C22.7858%2028.2858%2028.2863%2022.7853%2028.2863%2016.0001C28.2863%209.21486%2022.7858%203.71436%2016.0006%203.71436C9.21535%203.71436%203.71484%209.21486%203.71484%2016.0001C3.71484%2022.7853%209.21535%2028.2858%2016.0006%2028.2858Z%22%20stroke%3D%22black%22%2F%3E%3Cpath%20d%3D%22M27.1412%2016C27.1412%2022.1541%2022.1524%2027.1429%2015.9983%2027.1429C9.84427%2027.1429%204.85547%2022.1541%204.85547%2016C4.85547%209.84598%209.84427%204.85718%2015.9983%204.85718C22.1524%204.85718%2027.1412%209.84598%2027.1412%2016Z%22%20stroke%3D%22black%22%20stroke-width%3D%220.5%22%2F%3E%3Cpath%20fill-rule%3D%22evenodd%22%20clip-rule%3D%22evenodd%22%20d%3D%22M16.2167%2011.1743C15.9198%2011.1643%2015.6095%2011.1622%2015.2868%2011.1668V9.32056H13.8907V11.2217C13.1583%2011.2659%2012.3792%2011.3332%2011.5625%2011.4149V12.811H12.9586V18.8607H11.7952V20.4895H13.8893V22.5836H15.2854V20.4895H16.2161V22.5836H17.3795V20.4895C18.4654%2020.4119%2020.6836%2019.7915%2020.8698%2017.93C21.0559%2016.0686%2019.7064%2015.6032%2019.0083%2015.6032C19.5512%2015.3705%2020.544%2014.5328%2020.1717%2013.0436C19.9215%2012.043%2019.0072%2011.5204%2017.6128%2011.2984V9.32164H16.2167V11.1743ZM18.0737%2013.9723C18.0737%2012.8554%2016.2122%2012.7313%2015.2815%2012.8088V15.1356C16.2122%2015.2132%2018.0737%2015.0891%2018.0737%2013.9723ZM15.2826%2016.5322V18.8591C16.2133%2018.9366%2018.3075%2018.859%2018.3075%2017.6956C18.3075%2016.2994%2016.2133%2016.4547%2015.2826%2016.5322Z%22%20fill%3D%22black%22%2F%3E%3C%2Fg%3E%3Cdefs%3E%3CclipPath%20id%3D%22clip0_2351_779%22%3E%3Crect%20width%3D%2232%22%20height%3D%2232%22%20fill%3D%22white%22%2F%3E%3C%2FclipPath%3E%3C%2Fdefs%3E%3C%2Fsvg%3E";

pub struct Context {
    pub root: Account,
    pub tx_listener: Account,
    pub alice: Account,
    pub bob: Account,
    pub relayer: Account,
    pub charlie: Account,
    pub bridge_contract: Contract,
    pub nbtc_contract: Contract,
    pub chain_signatures_contract: Contract,
    pub btc_light_client_contract: Contract,
    pub dapp_contract: Contract,
}

impl Context {
    pub async fn new(worker: &Worker<Sandbox>) -> Self {
        Self::new_with_chain(worker, "BitcoinMainnet").await
    }

    pub async fn new_with_chain(worker: &Worker<Sandbox>, chain: &str) -> Self {
        let root = worker.root_account().unwrap();
        async fn compile_or_read(crate_dir: &str, fallback_res: &str) -> Vec<u8> {
            match compile_project(crate_dir).await {
                Ok(wasm) => wasm,
                Err(_) => std::fs::read(fallback_res).expect("read fallback wasm"),
            }
        }
        let (
            bridge_contract,
            nbtc_contract,
            chain_signatures_contract,
            btc_light_client_contract,
            dapp_contract,
            orchard_verifier_contract,
        ) = tokio::join!(
            async {
                let bridge = root
                    .create_subaccount("bridge")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap();
                // Prefer a test-enabled zcash artifact if present; else zcash-only; else compile
                let wasm = if let Ok(bytes) = std::fs::read("../../res/zcash_test.wasm") {
                    bytes
                } else if let Ok(bytes) = std::fs::read("../../res/zcash.wasm") {
                    bytes
                } else {
                        compile_or_read(
                            "../../contracts/satoshi-bridge",
                            "../../res/satoshi_bridge.wasm",
                        )
                        .await
                };
                bridge
                    .deploy(&wasm)
                    .await
                    .unwrap()
                    .unwrap()
            },
            async {
                let nbtc = root
                    .create_subaccount("nbtc")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap();
                let wasm = compile_or_read("../../contracts/nbtc", "../../res/nbtc.wasm").await;
                nbtc.deploy(&wasm)
                    .await
                    .unwrap()
                    .unwrap()
            },
            async {
                let wasm =
                    compile_or_read("../../contracts/mock-chain-signatures", "../../res/mock_chain_signatures.wasm").await;
                worker
                    .dev_deploy(&wasm)
                    .await
                    .unwrap()
            },
            async {
                let wasm =
                    compile_or_read("../../contracts/mock-btc-light-client", "../../res/mock_btc_light_client.wasm").await;
                worker
                    .dev_deploy(&wasm)
                    .await
                    .unwrap()
            },
            async {
                let nbtc = root
                    .create_subaccount("dapp")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap();
                let wasm =
                    compile_or_read("../../contracts/mock-dapp", "../../res/mock_dapp.wasm").await;
                nbtc.deploy(&wasm)
                    .await
                    .unwrap()
                    .unwrap()
            },
            async {
                // Deploy Orchard verifier contract
                let wasm = compile_project("../../contracts/orchard-verifier")
                    .await
                    .unwrap();
                let contract = worker.dev_deploy(&wasm).await.unwrap();
                // Initialize if needed
                contract
                    .call("new")
                    .args_json(json!({}))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap();
                contract
            },
        );

        let (tx_listener, alice, bob, relayer, charlie) = tokio::join!(
            async {
                root.create_subaccount("tx_listener")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap()
            },
            async {
                root.create_subaccount("alice")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap()
            },
            async {
                root.create_subaccount("bob")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap()
            },
            async {
                root.create_subaccount("relayer")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap()
            },
            async { worker.dev_create_account().await.unwrap() },
        );

        nbtc_contract
            .call("new")
            .args_json(json!({
                "controller": root.id(),
                "bridge_id": bridge_contract.id(),
                "name": "Near WTC".to_string(),
                "symbol": "NBTC".to_string(),
                "icon": Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                "decimals": 8u8,
            }))
            .transact()
            .await
            .unwrap()
            .unwrap();

        chain_signatures_contract
            .call("new")
            .args_json(json!({
                "public_key": "secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3",
            }))
            .transact()
            .await
            .unwrap()
            .unwrap();

        root.call(bridge_contract.id(), "new")
            .args_json(json!({
                "config": {
                    "chain": chain,
                    "chain_signatures_account_id": chain_signatures_contract.id(),
                    "nbtc_account_id": nbtc_contract.id(),
                    "btc_light_client_account_id": btc_light_client_contract.id(),
                    "confirmations_strategy": {
                        "10000000": 2,
                    },
                    "confirmations_delta": 1,
                    "extra_msg_confirmations_delta": 1,
                    "withdraw_bridge_fee": {
                        "fee_min": "50000",
                        "fee_rate": 0,
                        "protocol_fee_rate": 9000,
                    },
                    "deposit_bridge_fee": {
                        "fee_min": "0",
                        "fee_rate": 0,
                        "protocol_fee_rate": 9000,
                    },
                    "min_deposit_amount": "20000",
                    "min_withdraw_amount": "70000",
                    "min_change_amount": "0",
                    "max_change_amount": u128::MAX.to_string(),
                    "min_btc_gas_fee": "10000",
                    "max_btc_gas_fee": "50000",
                    "max_withdrawal_input_number": 10,
                    "max_change_number": 10,
                    "max_active_utxo_management_input_number": 2,
                    "max_active_utxo_management_output_number": 2,
                    "active_management_lower_limit": 0,
                    "active_management_upper_limit": u32::MAX,
                    "passive_management_lower_limit": 0,
                    "passive_management_upper_limit": u32::MAX,
                    "rbf_num_limit": 99,
                    "max_btc_tx_pending_sec": 3600 * 24,
                    "unhealthy_utxo_amount": 1000,
                    "orchard_verifier_account_id": if chain.contains("Zcash") { Some(orchard_verifier_contract.id()) } else { Option::<&AccountId>::None },
                    "expiry_height_gap": 1000,
                }
            }))
            .transact()
            .await
            .unwrap()
            .unwrap();
        root.call(
            bridge_contract.id(),
            "sync_chain_signatures_root_public_key",
        )
        .args_json(json!({}))
        .max_gas()
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await
        .unwrap()
        .unwrap();

        dapp_contract
            .call("new")
            .args_json(json!({}))
            .transact()
            .await
            .unwrap()
            .unwrap();

        Self {
            root,
            tx_listener,
            alice,
            bob,
            relayer,
            charlie,
            bridge_contract,
            nbtc_contract,
            chain_signatures_contract,
            btc_light_client_contract,
            dapp_contract,
        }
    }

    pub fn get_account_by_name(&self, user_name: &str) -> &Account {
        match user_name {
            "root" => &self.root,
            "tx_listener" => &self.tx_listener,
            "alice" => &self.alice,
            "bob" => &self.bob,
            "relayer" => &self.relayer,
            "charlie" => &self.charlie,
            "bridge" => self.bridge_contract.as_account(),
            "nbtc" => self.nbtc_contract.as_account(),
            "chain_signatures" => self.chain_signatures_contract.as_account(),
            "btc_light_client" => self.btc_light_client_contract.as_account(),
            "dapp" => self.dapp_contract.as_account(),
            _ => {
                println!("input {user_name}");
                unimplemented!()
            }
        }
    }

    pub async fn near_balance_by_account(
        &self,
        worker: &Worker<Sandbox>,
        account_id: &AccountId,
    ) -> u128 {
        match worker.view_account(account_id).await {
            Ok(a) => a.balance.as_yoctonear(),
            Err(_) => 0,
        }
    }
}

// api of nbtc
impl Context {
    pub async fn set_metadata(&self, icon: &str) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.nbtc_contract.id(), "set_metadata")
            .args_json(json!({
                "icon": icon,
            }))
            .deposit(NearToken::from_yoctonear(1))
            .max_gas()
            .transact()
            .await
    }

    pub async fn ft_metadata(&self) -> Result<FungibleTokenMetadata> {
        self.nbtc_contract
            .call("ft_metadata")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<FungibleTokenMetadata>()
    }

    pub async fn ft_total_supply(&self) -> Result<U128> {
        self.nbtc_contract
            .call("ft_total_supply")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<U128>()
    }

    pub async fn ft_balance_of(&self, user_name: &str) -> Result<U128> {
        self.nbtc_contract
            .call("ft_balance_of")
            .args_json(json!({
                "account_id": self.get_account_by_name(user_name).id()
            }))
            .view()
            .await
            .unwrap()
            .json::<U128>()
    }

    pub async fn ft_balance_of_by_account_id(&self, account_id: &AccountId) -> Result<U128> {
        self.nbtc_contract
            .call("ft_balance_of")
            .args_json(json!({
                "account_id": account_id
            }))
            .view()
            .await
            .unwrap()
            .json::<U128>()
    }

    pub async fn ft_transfer(
        &self,
        user_name: &str,
        receiver_id: &str,
        amount: u128,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user_name)
            .call(self.nbtc_contract.id(), "ft_transfer")
            .args_json(json!({
                "receiver_id": self.get_account_by_name(receiver_id).id(),
                "amount": amount.to_string(),
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn ft_transfer_call(
        &self,
        user_name: &str,
        receiver_id: &str,
        amount: u128,
        msg: String,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user_name)
            .call(self.nbtc_contract.id(), "ft_transfer_call")
            .args_json(json!({
                "receiver_id": self.get_account_by_name(receiver_id).id(),
                "amount": amount.to_string(),
                "msg": msg
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn do_withdraw(
        &self,
        user_name: &str,
        receiver_id: &str,
        amount: u128,
        msg: TokenReceiverMessage,
    ) -> Result<ExecutionFinalResult> {
        self.ft_transfer_call(
            user_name,
            receiver_id,
            amount,
            near_sdk::serde_json::to_string(&msg).unwrap(),
        )
        .await
    }

    pub async fn storage_deposit(
        &self,
        contract_id: &str,
        user: &str,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user)
            .call(
                self.get_account_by_name(contract_id).id(),
                "storage_deposit",
            )
            .args_json(json!({
                "registration_only": true
            }))
            .max_gas()
            .deposit(NearToken::from_near(1))
            .transact()
            .await
    }

    pub async fn storage_balance_of(&self, contract_id: &str, user: &str) -> Result<Value> {
        let user = self.get_account_by_name(user);
        user.call(
            self.get_account_by_name(contract_id).id(),
            "storage_balance_of",
        )
        .args_json(json!({"account_id": user.id()}))
        .view()
        .await
        .unwrap()
        .json::<Value>()
    }

    pub async fn storage_balance_of_by_account_id(
        &self,
        contract_id: &str,
        account_id: &AccountId,
    ) -> Result<Value> {
        self.root
            .call(
                self.get_account_by_name(contract_id).id(),
                "storage_balance_of",
            )
            .args_json(json!({"account_id": account_id}))
            .view()
            .await
            .unwrap()
            .json::<Value>()
    }
}

impl Context {
    pub async fn get_metadata(&self) -> Result<Metadata> {
        self.bridge_contract
            .call("get_metadata")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<Metadata>()
    }

    pub async fn get_bridge_config(&self) -> Result<satoshi_bridge::Config> {
        self.bridge_contract
            .call("get_config")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<satoshi_bridge::Config>()
    }

    pub async fn bridge_acl_add_super_admin(
        &self,
        caller: &str,
        account_id: &AccountId,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(caller)
            .call(self.bridge_contract.id(), "acl_add_super_admin")
            .args_json(json!({
                "account_id": account_id
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn bridge_acl_grant_role(
        &self,
        caller: &str,
        role: &str,
        account_id: &AccountId,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(caller)
            .call(self.bridge_contract.id(), "acl_grant_role")
            .args_json(json!({
                "role": role,
                "account_id": account_id
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn bridge_add_super_admin(
        &self,
        caller: &str,
        account_id: &AccountId,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(caller)
            .call(self.bridge_contract.id(), "add_super_admin")
            .args_json(json!({
                "account_id": account_id
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn bridge_remove_super_admin(
        &self,
        caller: &str,
        account_id: &AccountId,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(caller)
            .call(self.bridge_contract.id(), "remove_super_admin")
            .args_json(json!({
                "account_id": account_id
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn bridge_pa_pause_feature(
        &self,
        caller: &str,
        key: &str,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(caller)
            .call(self.bridge_contract.id(), "pa_pause_feature")
            .args_json(json!({
                "key": key
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn bridge_pa_unpause_feature(
        &self,
        caller: &str,
        key: &str,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(caller)
            .call(self.bridge_contract.id(), "pa_unpause_feature")
            .args_json(json!({
                "key": key
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn bridge_pa_all_paused(&self) -> Result<Option<HashSet<String>>> {
        self.root
            .call(self.bridge_contract.id(), "pa_all_paused")
            .view()
            .await
            .unwrap()
            .json::<Option<HashSet<String>>>()
    }

    pub async fn extend_post_action_receiver_id_white_list(
        &self,
        receiver_ids: Vec<AccountId>,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(
                self.bridge_contract.id(),
                "extend_post_action_receiver_id_white_list",
            )
            .args_json(json!({
                "receiver_ids": receiver_ids
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn set_withdraw_bridge_fee(
        &self,
        fee_min: u128,
        fee_rate: u32,
        protocol_fee_rate: u32,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "set_withdraw_bridge_fee")
            .args_json(json!({
                "withdraw_bridge_fee": {
                    "fee_min": fee_min.to_string(),
                    "fee_rate": fee_rate,
                    "protocol_fee_rate": protocol_fee_rate,
                },
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn set_deposit_bridge_fee(
        &self,
        fee_min: u128,
        fee_rate: u32,
        protocol_fee_rate: u32,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "set_deposit_bridge_fee")
            .args_json(json!({
                "deposit_bridge_fee": {
                    "fee_min": fee_min.to_string(),
                    "fee_rate": fee_rate,
                    "protocol_fee_rate": protocol_fee_rate,
                },
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn withdraw_protocol_fee(
        &self,
        amount: Option<u128>,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "withdraw_protocol_fee")
            .args_json(json!({
                "amount": amount.map(U128),
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn set_active_management_limit(
        &self,
        active_management_lower_limit: u32,
        active_management_upper_limit: u32,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "set_active_management_limit")
            .args_json(json!({
                "active_management_lower_limit": active_management_lower_limit,
                "active_management_upper_limit": active_management_upper_limit,
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn set_passive_management_limit(
        &self,
        passive_management_lower_limit: u32,
        passive_management_upper_limit: u32,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "set_passive_management_limit")
            .args_json(json!({
                "passive_management_lower_limit": passive_management_lower_limit,
                "passive_management_upper_limit": passive_management_upper_limit,
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn set_btc_gas_fee_valid_range(
        &self,
        min_btc_gas_fee: u128,
        max_btc_gas_fee: u128,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "set_btc_gas_fee_valid_range")
            .args_json(json!({
                "min_btc_gas_fee": min_btc_gas_fee.to_string(),
                "max_btc_gas_fee": max_btc_gas_fee.to_string(),
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn set_max_btc_tx_pending_sec(
        &self,
        max_btc_tx_pending_sec: u32,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "set_max_btc_tx_pending_sec")
            .args_json(json!({
                "max_btc_tx_pending_sec": max_btc_tx_pending_sec,
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn set_nbtc_account_id(&self) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "set_nbtc_account_id")
            .args_json(json!({
                "nbtc_account_id": self.get_account_by_name("nbtc").id(),
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn active_utxo_management(
        &self,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "active_utxo_management")
            .args_json(json!({
                "input": input,
                "output": output,
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn active_utxo_management_rbf(
        &self,
        original_btc_pending_verify_id: &String,
        output: Vec<TxOut>,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "active_utxo_management_rbf")
            .args_json(json!({
                "original_btc_pending_verify_id": original_btc_pending_verify_id,
                "output": output,
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn cancel_active_utxo_management(
        &self,
        original_btc_pending_verify_id: &String,
        output: Vec<TxOut>,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "cancel_active_utxo_management")
            .args_json(json!({
                "original_btc_pending_verify_id": original_btc_pending_verify_id,
                "output": output,
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn cancel_withdraw(
        &self,
        original_btc_pending_verify_id: &String,
        output: Vec<TxOut>,
    ) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.bridge_contract.id(), "cancel_withdraw")
            .args_json(json!({
                "original_btc_pending_verify_id": original_btc_pending_verify_id,
                "output": output,
            }))
            .max_gas()
            .deposit(NearToken::from_yoctonear(1))
            .transact()
            .await
    }

    pub async fn verify_deposit(
        &self,
        user: &str,
        deposit_msg: DepositMsg,
        tx_bytes: Vec<u8>,
        vout: u32,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user)
            .call(self.bridge_contract.id(), "verify_deposit")
            .args_json(json!({
                "deposit_msg": deposit_msg,
                "tx_bytes": tx_bytes,
                "vout": vout,
                "tx_block_blockhash": tx_block_blockhash,
                "tx_index": tx_index,
                "merkle_proof": merkle_proof,
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn sign_btc_transaction(
        &self,
        user: &str,
        btc_pending_sign_id: &str,
        sign_index: usize,
        key_version: u32,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user)
            .call(self.bridge_contract.id(), "sign_btc_transaction")
            .args_json(json!({
                "btc_pending_sign_id": btc_pending_sign_id,
                "sign_index": sign_index,
                "key_version": key_version,
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn verify_active_utxo_management(
        &self,
        user: &str,
        tx_id: &str,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user)
            .call(self.bridge_contract.id(), "verify_active_utxo_management")
            .args_json(json!({
                "tx_id": tx_id,
                "tx_block_blockhash": tx_block_blockhash,
                "tx_index": tx_index,
                "merkle_proof": merkle_proof,
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn verify_withdraw(
        &self,
        user: &str,
        tx_id: &str,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user)
            .call(self.bridge_contract.id(), "verify_withdraw")
            .args_json(json!({
                "tx_id": tx_id,
                "tx_block_blockhash": tx_block_blockhash,
                "tx_index": tx_index,
                "merkle_proof": merkle_proof,
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn clear_invalid_pending_verify_rbf(
        &self,
        user: &str,
        btc_pending_verify_id: &str,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user)
            .call(
                self.bridge_contract.id(),
                "clear_invalid_pending_verify_rbf",
            )
            .args_json(json!({
                "btc_pending_verify_id": btc_pending_verify_id,
            }))
            .max_gas()
            .transact()
            .await
    }

    pub async fn get_user_deposit_address(&self, deposit_msg: DepositMsg) -> Result<String> {
        self.bridge_contract
            .call("get_user_deposit_address")
            .args_json(json!({
                "deposit_msg": deposit_msg,
            }))
            .view()
            .await
            .unwrap()
            .json::<String>()
    }

    pub async fn get_near_user_dapp_deposit_address(
        &self,
        near_account_id: &AccountId,
        deposit_operation: String,
    ) -> Result<String> {
        self.bridge_contract
            .call("get_near_user_dapp_deposit_address")
            .args_json(json!({
                "near_account_id": near_account_id,
                "deposit_operation": deposit_operation,
            }))
            .view()
            .await
            .unwrap()
            .json::<String>()
    }

    pub async fn get_btc_user_deposit_address(&self, btc_public_key: &str) -> Result<String> {
        self.bridge_contract
            .call("get_btc_user_deposit_address")
            .args_json(json!({
                "btc_public_key": btc_public_key,
            }))
            .view()
            .await
            .unwrap()
            .json::<String>()
    }

    pub async fn get_btc_user_dapp_deposit_address(
        &self,
        btc_public_key: &str,
        deposit_operation: String,
    ) -> Result<String> {
        self.bridge_contract
            .call("get_btc_user_dapp_deposit_address")
            .args_json(json!({
                "btc_public_key": btc_public_key,
                "deposit_operation": deposit_operation,
            }))
            .view()
            .await
            .unwrap()
            .json::<String>()
    }

    pub async fn get_change_address(&self) -> Result<String> {
        self.bridge_contract
            .call("get_change_address")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<String>()
    }

    pub async fn get_account(&self, user: &str) -> Result<Option<satoshi_bridge::Account>> {
        self.bridge_contract
            .call("get_account")
            .args_json(json!({
                "account_id": self.get_account_by_name(user).id()
            }))
            .view()
            .await
            .unwrap()
            .json::<Option<satoshi_bridge::Account>>()
    }

    pub async fn get_bridge_account_by_account_id(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<satoshi_bridge::Account>> {
        self.bridge_contract
            .call("get_account")
            .args_json(json!({
                "account_id": account_id
            }))
            .view()
            .await
            .unwrap()
            .json::<Option<satoshi_bridge::Account>>()
    }

    pub async fn get_accounts_paged(&self) -> Result<HashMap<AccountId, satoshi_bridge::Account>> {
        self.bridge_contract
            .call("get_accounts_paged")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<HashMap<AccountId, satoshi_bridge::Account>>()
    }

    pub async fn get_utxos_paged(&self) -> Result<HashMap<String, UTXO>> {
        self.bridge_contract
            .call("get_utxos_paged")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<HashMap<String, UTXO>>()
    }

    pub async fn get_unavailable_utxos_paged(&self) -> Result<HashMap<String, UTXO>> {
        self.bridge_contract
            .call("get_unavailable_utxos_paged")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<HashMap<String, UTXO>>()
    }

    pub async fn get_btc_pending_infos_paged(&self) -> Result<HashMap<String, BTCPendingInfo>> {
        self.bridge_contract
            .call("get_btc_pending_infos_paged")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<HashMap<String, BTCPendingInfo>>()
    }

    pub async fn withdraw_gas_token(
        &self,
        user: &str,
        gas_token_id: &AccountId,
        amount: Option<u128>,
    ) -> Result<ExecutionFinalResult> {
        self.get_account_by_name(user)
            .call(self.bridge_contract.id(), "withdraw_gas_token")
            .args_json(json!({
                "gas_token_id": gas_token_id,
                "amount": amount.map(U128),
            }))
            .max_gas()
            .transact()
            .await
    }
}

pub struct UpgradeContext {
    pub root: Account,
    pub previous_satoshi_bridge_contract: Contract,
    pub previous_nbtc_contract: Contract,
}

impl UpgradeContext {
    pub async fn new(worker: &Worker<Sandbox>) -> Self {
        let root = worker.root_account().unwrap();
        let (
            previous_satoshi_bridge_contract,
            previous_nbtc_contract,
            chain_signatures_contract,
            btc_light_client_contract,
        ) = tokio::join!(
            async {
                worker
                    .dev_deploy(&std::fs::read("../../res/satoshi_bridge.wasm").unwrap())
                    .await
                    .unwrap()
            },
            async {
                let nbtc = root
                    .create_subaccount("nbtc")
                    .initial_balance(NearToken::from_near(100))
                    .transact()
                    .await
                    .unwrap()
                    .unwrap();
                nbtc.deploy(&std::fs::read("../../res/nbtc.wasm").unwrap())
                    .await
                    .unwrap()
                    .unwrap()
            },
            async {
                worker
                    .dev_deploy(&std::fs::read("../../res/mock_chain_signatures.wasm").unwrap())
                    .await
                    .unwrap()
            },
            async {
                worker
                    .dev_deploy(&std::fs::read("../../res/mock_btc_light_client.wasm").unwrap())
                    .await
                    .unwrap()
            },
        );

        root.call(previous_satoshi_bridge_contract.id(), "new")
            .args_json(json!({
                "config": {
                    "chain": "BitcoinMainnet",
                    "chain_signatures_account_id": chain_signatures_contract.id(),
                    "nbtc_account_id": previous_nbtc_contract.id(),
                    "btc_light_client_account_id": btc_light_client_contract.id(),
                    "confirmations_strategy": {
                        "10000000": 2,
                    },
                    "confirmations_delta": 1,
                    "extra_msg_confirmations_delta": 1,
                    "withdraw_bridge_fee": {
                        "fee_min": "50000",
                        "fee_rate": 0,
                        "protocol_fee_rate": 9000,
                    },
                    "deposit_bridge_fee": {
                        "fee_min": "0",
                        "fee_rate": 0,
                        "protocol_fee_rate": 9000,
                    },
                    "min_deposit_amount": "20000",
                    "min_withdraw_amount": "70000",
                    "min_change_amount": "0",
                    "max_change_amount": u128::MAX.to_string(),
                    "min_btc_gas_fee": "10000",
                    "max_btc_gas_fee": "50000",
                    "max_withdrawal_input_number": 10,
                    "max_change_number": 10,
                    "max_active_utxo_management_input_number": 2,
                    "max_active_utxo_management_output_number": 2,
                    "active_management_lower_limit": 0,
                    "active_management_upper_limit": u32::MAX,
                    "passive_management_lower_limit": 0,
                    "passive_management_upper_limit": u32::MAX,
                    "rbf_num_limit": 99,
                    "max_btc_tx_pending_sec": 3600 * 24,
                    "unhealthy_utxo_amount": 1000,
                }
            }))
            .transact()
            .await
            .unwrap()
            .unwrap();
        root.call(
            previous_satoshi_bridge_contract.id(),
            "sync_chain_signatures_root_public_key",
        )
        .args_json(json!({}))
        .max_gas()
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await
        .unwrap()
        .unwrap();
        root.call(previous_nbtc_contract.id(), "new")
            .args_json(json!({
                "controller": root.id(),
                "bridge_id": previous_satoshi_bridge_contract.id(),
            }))
            .transact()
            .await
            .unwrap()
            .unwrap();
        Self {
            root,
            previous_satoshi_bridge_contract,
            previous_nbtc_contract,
        }
    }

    pub async fn upgrade_satoshi_bridge(&self, wasm_path: &str) -> Result<ExecutionFinalResult> {
        let _ = self
            .root
            .call(self.previous_satoshi_bridge_contract.id(), "up_stage_code")
            .args_borsh(std::fs::read(wasm_path).unwrap())
            .max_gas()
            .transact()
            .await
            .unwrap();

        let staged_code_hash: near_sdk::CryptoHash = self
            .root
            .call(
                self.previous_satoshi_bridge_contract.id(),
                "up_staged_code_hash",
            )
            .view()
            .await
            .unwrap()
            .json::<Option<near_sdk::CryptoHash>>()
            .unwrap()
            .unwrap();

        self.root
            .call(self.previous_satoshi_bridge_contract.id(), "up_deploy_code")
            .args_json(
                json!({"hash":  base64::engine::general_purpose::STANDARD.encode(staged_code_hash),
                "function_call_args": Some(near_plugins::upgradable::FunctionCallArgs{
                    function_name: "migrate_state".to_string(),
                    arguments: vec![],
                    amount: NearToken::from_near(0),
                    gas: Gas::from_tgas(20)
                })}),
            )
            .max_gas()
            .transact()
            .await
    }

    pub async fn upgrade_nbtc(&self, wasm_path: &str) -> Result<ExecutionFinalResult> {
        self.root
            .call(self.previous_nbtc_contract.id(), "upgrade_and_migrate")
            .args(std::fs::read(wasm_path).unwrap())
            .max_gas()
            .transact()
            .await
    }

    pub async fn get_satoshi_bridge_version(&self) -> Result<String> {
        self.previous_satoshi_bridge_contract
            .call("get_version")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<String>()
    }

    pub async fn get_nbtc_version(&self) -> Result<String> {
        self.previous_nbtc_contract
            .call("version")
            .args_json(json!({}))
            .view()
            .await
            .unwrap()
            .json::<String>()
    }
}
