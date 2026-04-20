use crate::{
    env, near, network, network::Address, require, u128_dec_format, u128_dec_format_option,
    AccountId, Contract, HashMap, PublicKey, ScriptBuf,
};

pub const MAX_RATIO: u32 = 10000;

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct BridgeFee {
    #[serde(with = "u128_dec_format")]
    pub fee_min: u128,
    pub fee_rate: u32,
    pub protocol_fee_rate: u32,
}

impl BridgeFee {
    pub fn assert_valid(&self) {
        require!(self.fee_rate < MAX_RATIO, "Invalid fee_rate");
        require!(
            self.protocol_fee_rate <= MAX_RATIO,
            "Invalid protocol_fee_rate"
        );
    }

    pub fn get_fee(&self, amount: u128) -> u128 {
        std::cmp::max(
            amount * u128::from(self.fee_rate) / u128::from(MAX_RATIO),
            self.fee_min,
        )
    }

    pub fn get_protocol_and_relayer_fee(&self, fee_amount: u128) -> (u128, u128) {
        let protocol_fee = fee_amount * u128::from(self.protocol_fee_rate) / u128::from(MAX_RATIO);
        let relayer_fee = fee_amount - protocol_fee;
        (protocol_fee, relayer_fee)
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct Config {
    // The chain id: BitconMainnet/BitcoinTestnet/ZcashMainnet/ZcashTestnet etc
    pub chain: network::Chain,
    // The account id of btc light client contract
    pub btc_light_client_account_id: AccountId,
    // The account id of nbtc contract
    pub nbtc_account_id: AccountId,
    // The account id of chain signatures contract
    pub chain_signatures_account_id: AccountId,
    // The root public key of chain signatures contract
    pub chain_signatures_root_public_key: Option<PublicKey>,
    // The change address of BTC transaction
    pub change_address: Option<String>,
    // Satoshi upper limit for amount checks -> confirmations
    pub confirmations_strategy: HashMap<String, u8>,
    // The number of confirmations that need to be increased when a relayer not on the whitelist performs a verify.
    pub confirmations_delta: u8,
    // The number of confirmations that need to be increased when a relayer not on the extra msg whitelist performs a verify.
    pub extra_msg_confirmations_delta: u8,
    // Used to calculate the deposit fee.
    pub deposit_bridge_fee: BridgeFee,
    // Used to calculate the withdraw fee.
    pub withdraw_bridge_fee: BridgeFee,
    // The min amount must be met during verify_deposit, otherwise NBTC will not be minted for the user.
    #[serde(with = "u128_dec_format")]
    pub min_deposit_amount: u128,
    // The minimum amount allowed for the user to withdraw.
    #[serde(with = "u128_dec_format")]
    pub min_withdraw_amount: u128,
    // The minimum value requirement that change address must satisfy in BTC transaction.
    #[serde(with = "u128_dec_format")]
    pub min_change_amount: u128,
    // Used to limit the maximum value of change in specific situations.
    #[serde(with = "u128_dec_format")]
    pub max_change_amount: u128,
    // The min gas fee applicable for Bitcoin transactions
    #[serde(with = "u128_dec_format")]
    pub min_btc_gas_fee: u128,
    // The max gas fee applicable for Bitcoin transactions
    #[serde(with = "u128_dec_format")]
    pub max_btc_gas_fee: u128,
    // The maximum number of inputs that can be used for a Withdraw.
    pub max_withdrawal_input_number: u8,
    // The maximum amount of change allowed during a Withdraw.
    pub max_change_number: u8,
    // The maximum number of inputs allowed during active UTXO management.
    pub max_active_utxo_management_input_number: u8,
    // The maximum number of outputs allowed during active UTXO management.
    pub max_active_utxo_management_output_number: u8,
    // When the number of UTXOs in the protocol is less than this configuration, UTXO management can be actively initiated.
    // The number of inputs in the managed PSBT must be less than the number of outputs.
    pub active_management_lower_limit: u32,
    // When the number of UTXOs in the protocol is greater than this configuration, UTXO management can be actively initiated.
    // The number of inputs in the managed PSBT must be greater than the number of outputs.
    pub active_management_upper_limit: u32,
    // When the number of UTXOs in the protocol is less than this configuration, passive UTXO management will be triggered,
    // requiring that the number of inputs must be less than the number of changes.
    pub passive_management_lower_limit: u32,
    // When the number of UTXOs in the protocol is greater than this configuration, passive UTXO management will be triggered,
    // requiring that the number of inputs must be greater than the number of changes.
    pub passive_management_upper_limit: u32,
    // The maximum number of transactions allowed to initiate RBF
    pub rbf_num_limit: u8,
    // If the transaction exceeds this configuration and has not been verified, the protocol will be allowed to cancel the transaction.
    pub max_btc_tx_pending_sec: u32,
    // UTXOs less than or equal to this amount are allowed to be merged through active management.
    pub unhealthy_utxo_amount: u64,
    // Timelock in seconds before a refund request can be executed.
    pub refund_timelock_sec: u64,
    #[cfg(feature = "zcash")]
    pub expiry_height_gap: u32,
}

impl Config {
    pub fn assert_valid(&self) {
        let confirmations_valid_range = 2..=100;
        require!(
            self.confirmations_strategy
                .values()
                .all(|v| confirmations_valid_range.contains(v)),
            "Invalid confirmations_strategy"
        );
        self.deposit_bridge_fee.assert_valid();
        self.withdraw_bridge_fee.assert_valid();
        require!(
            self.min_change_amount < self.max_change_amount,
            "min_change_amount must be less than max_change_amount"
        );
        require!(
            self.min_btc_gas_fee < self.max_btc_gas_fee,
            "min_btc_gas_fee must be less than max_btc_gas_fee"
        );
        require!(
            self.active_management_lower_limit < self.active_management_upper_limit,
            "active_management_lower_limit must be less than active_management_upper_limit"
        );
        require!(
            self.passive_management_lower_limit < self.passive_management_upper_limit,
            "passive_management_lower_limit must be less than passive_management_upper_limit"
        );
        require!(
            u128::from(self.unhealthy_utxo_amount) > self.min_change_amount,
            "unhealthy_utxo_amount must be greater than min_change_amount"
        );
    }

    pub fn get_change_script_pubkey(&self) -> ScriptBuf {
        self.string_to_script_pubkey(
            self.change_address
                .as_ref()
                .expect("ERR_CONFIG: change_address not configured"),
        )
    }

    pub fn string_to_script_pubkey(&self, address_string: &str) -> ScriptBuf {
        let chain = self.get_utxo_network();

        Address::parse(address_string, chain)
            .unwrap_or_else(|e| env::panic_str(&format!("{address_string}: {e}")))
            .script_pubkey()
            .expect("Failed to get script pubkey")
    }

    pub fn get_utxo_network(&self) -> network::Chain {
        self.chain.clone()
    }

    pub fn get_confirmations(&self, satoshi_amount: u128) -> u64 {
        require!(
            !self.confirmations_strategy.is_empty(),
            "confirmations_strategy is empty"
        );
        // The key is constrained to U64 during assignment, so it won't panic.
        let mut keys = self
            .confirmations_strategy
            .keys()
            .map(|k| k.parse::<u128>().unwrap())
            .collect::<Vec<_>>();
        keys.sort_unstable();
        for key in &keys {
            if *key > satoshi_amount {
                return u64::from(*self.confirmations_strategy.get(&key.to_string()).unwrap());
            }
        }
        let max_key = keys.last().unwrap();
        u64::from(
            *self
                .confirmations_strategy
                .get(&max_key.to_string())
                .unwrap(),
        )
    }
}

#[near(serializers = [json])]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct ConfigUpdate {
    pub btc_light_client_account_id: Option<AccountId>,
    pub nbtc_account_id: Option<AccountId>,
    pub confirmations_delta: Option<u8>,
    pub extra_msg_confirmations_delta: Option<u8>,
    pub deposit_bridge_fee: Option<BridgeFee>,
    pub withdraw_bridge_fee: Option<BridgeFee>,
    #[serde(with = "u128_dec_format_option")]
    #[serde(default)]
    pub min_deposit_amount: Option<u128>,
    #[serde(with = "u128_dec_format_option")]
    #[serde(default)]
    pub min_withdraw_amount: Option<u128>,
    #[serde(with = "u128_dec_format_option")]
    #[serde(default)]
    pub min_change_amount: Option<u128>,
    #[serde(with = "u128_dec_format_option")]
    #[serde(default)]
    pub max_change_amount: Option<u128>,
    #[serde(with = "u128_dec_format_option")]
    #[serde(default)]
    pub min_btc_gas_fee: Option<u128>,
    #[serde(with = "u128_dec_format_option")]
    #[serde(default)]
    pub max_btc_gas_fee: Option<u128>,
    pub max_withdrawal_input_number: Option<u8>,
    pub max_change_number: Option<u8>,
    pub max_active_utxo_management_input_number: Option<u8>,
    pub max_active_utxo_management_output_number: Option<u8>,
    pub active_management_lower_limit: Option<u32>,
    pub active_management_upper_limit: Option<u32>,
    pub passive_management_lower_limit: Option<u32>,
    pub passive_management_upper_limit: Option<u32>,
    pub rbf_num_limit: Option<u8>,
    pub max_btc_tx_pending_sec: Option<u32>,
    pub unhealthy_utxo_amount: Option<u64>,
    pub refund_timelock_sec: Option<u64>,
}

impl ConfigUpdate {
    pub fn apply(self, config: &mut Config) {
        macro_rules! set_if_some {
            ($field:ident) => {
                if let Some(v) = self.$field {
                    config.$field = v;
                }
            };
        }
        set_if_some!(btc_light_client_account_id);
        set_if_some!(nbtc_account_id);
        set_if_some!(confirmations_delta);
        set_if_some!(extra_msg_confirmations_delta);
        set_if_some!(deposit_bridge_fee);
        set_if_some!(withdraw_bridge_fee);
        set_if_some!(min_deposit_amount);
        set_if_some!(min_withdraw_amount);
        set_if_some!(min_change_amount);
        set_if_some!(max_change_amount);
        set_if_some!(min_btc_gas_fee);
        set_if_some!(max_btc_gas_fee);
        set_if_some!(max_withdrawal_input_number);
        set_if_some!(max_change_number);
        set_if_some!(max_active_utxo_management_input_number);
        set_if_some!(max_active_utxo_management_output_number);
        set_if_some!(active_management_lower_limit);
        set_if_some!(active_management_upper_limit);
        set_if_some!(passive_management_lower_limit);
        set_if_some!(passive_management_upper_limit);
        set_if_some!(rbf_num_limit);
        set_if_some!(max_btc_tx_pending_sec);
        set_if_some!(unhealthy_utxo_amount);
        set_if_some!(refund_timelock_sec);

        config.assert_valid();
    }
}

impl Contract {
    pub fn internal_mut_config(&mut self) -> &mut Config {
        self.data_mut()
            .config
            .get_mut()
            .as_mut()
            .expect("ERR_CONFIG: contract not initialized")
    }

    pub fn internal_config(&self) -> &Config {
        self.data()
            .config
            .get()
            .as_ref()
            .expect("ERR_CONFIG: contract not initialized")
    }

    pub fn get_confirmations(&self, config: &Config, satoshi_amount: u128) -> u64 {
        if self
            .data()
            .relayer_white_list
            // Use predecessor_account_id to support both users and proxy protocols.
            .contains(&env::predecessor_account_id())
        {
            config.get_confirmations(satoshi_amount)
        } else {
            config.get_confirmations(satoshi_amount) + u64::from(config.confirmations_delta)
        }
    }

    pub fn get_extra_msg_confirmations(&self, config: &Config, satoshi_amount: u128) -> u64 {
        if self
            .data()
            .extra_msg_relayer_white_list
            .contains(&env::predecessor_account_id())
        {
            config.get_confirmations(satoshi_amount)
        } else {
            config.get_confirmations(satoshi_amount)
                + u64::from(config.extra_msg_confirmations_delta)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_config_update_changes_only_specified_field() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());

        let setup: ConfigUpdate =
            serde_json::from_str(r#"{ "min_change_amount": "500" }"#).unwrap();
        unit_env.contract.update_config(setup);

        let config_before = unit_env.contract.internal_config().clone();
        assert_ne!(config_before.min_change_amount, 0);

        let update: ConfigUpdate =
            serde_json::from_str(r#"{ "min_deposit_amount": "21000" }"#).unwrap();
        unit_env.contract.update_config(update);

        let config_after = unit_env.contract.internal_config();

        assert_eq!(config_after.min_deposit_amount, 21000);
        assert_eq!(config_after.min_change_amount, 500);
    }
}
