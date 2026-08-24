use crate::{
    env, near, network, network::Address, require, u128_dec_format, u128_dec_format_option,
    AccountId, Contract, HashMap, PublicKey, ScriptBuf,
};

pub const MAX_RATIO: u32 = 10000;

pub const DEFAULT_REFUND_TIMELOCK_SEC: u64 = 2 * 24 * 3600;
pub const DEFAULT_UNSAFE_REFUND_TIMELOCK_SEC: u64 = 14 * 24 * 3600;

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
    // Timelock for refunds where `deposit_msg.refund_address` is pre-authorized.
    pub refund_timelock_sec: u64,
    // Timelock for refunds where the refund address comes from the request caller
    // (`deposit_msg.refund_address` was None). Must be >= `refund_timelock_sec`.
    pub unsafe_refund_timelock_sec: u64,
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
        let mut tiers = self
            .confirmations_strategy
            .iter()
            .map(|(bound, confirmations)| {
                (
                    bound
                        .parse::<u128>()
                        .unwrap_or_else(|_| env::panic_str("Invalid confirmations_strategy key")),
                    *confirmations,
                )
            })
            .collect::<Vec<_>>();
        tiers.sort_unstable_by_key(|(bound, _)| *bound);
        require!(
            tiers.windows(2).all(|pair| pair[0].1 <= pair[1].1),
            "confirmations_strategy must be non-decreasing"
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
        require!(
            self.refund_timelock_sec <= self.unsafe_refund_timelock_sec,
            "refund_timelock_sec must be <= unsafe_refund_timelock_sec"
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

    /// scriptPubKey for a withdrawal target address, or `None` when the address has no
    /// transparent receiver (e.g. a shielded-only Zcash unified address that carries only
    /// Sapling/Orchard receivers). Such a recipient is paid via the Orchard bundle, so the
    /// withdrawal's transparent outputs are all change and there is no target scriptPubKey
    /// to match against. The address must still be parseable for the configured chain; an
    /// unparseable address panics, matching `string_to_script_pubkey`.
    pub fn target_script_pubkey(&self, address_string: &str) -> Option<ScriptBuf> {
        let chain = self.get_utxo_network();

        Address::parse(address_string, chain)
            .unwrap_or_else(|e| env::panic_str(&format!("{address_string}: {e}")))
            .script_pubkey()
            .ok()
    }

    pub fn get_utxo_network(&self) -> network::Chain {
        self.chain.clone()
    }

    pub fn sorted_confirmations_tiers(&self) -> Vec<(u128, u64)> {
        require!(
            !self.confirmations_strategy.is_empty(),
            "confirmations_strategy is empty"
        );
        // The key is constrained to U64 during assignment, so it won't panic.
        let mut tiers = self
            .confirmations_strategy
            .iter()
            .map(|(bound, confirmations)| {
                (bound.parse::<u128>().unwrap(), u64::from(*confirmations))
            })
            .collect::<Vec<_>>();
        tiers.sort_unstable();
        tiers
    }

    pub fn tier_confirmations(tiers: &[(u128, u64)], satoshi_amount: u128) -> u64 {
        tiers
            .iter()
            .find(|(bound, _)| *bound > satoshi_amount)
            .or_else(|| tiers.last())
            .map_or_else(
                || env::panic_str("confirmations_strategy is empty"),
                |(_, confirmations)| *confirmations,
            )
    }

    pub fn get_confirmations(&self, satoshi_amount: u128) -> u64 {
        Self::tier_confirmations(&self.sorted_confirmations_tiers(), satoshi_amount)
    }

    pub fn max_tier_confirmations(&self) -> u8 {
        self.confirmations_strategy
            .values()
            .max()
            .copied()
            .unwrap_or(0)
    }

    pub fn max_required_confirmations(&self) -> u64 {
        u64::from(self.max_tier_confirmations())
            + u64::from(std::cmp::max(
                self.confirmations_delta,
                self.extra_msg_confirmations_delta,
            ))
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
    pub unsafe_refund_timelock_sec: Option<u64>,
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
        set_if_some!(unsafe_refund_timelock_sec);

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

    pub(crate) fn relayer_delta(&self, relayer_account_id: &AccountId) -> u64 {
        if self.data().relayer_white_list.contains(relayer_account_id) {
            0
        } else {
            u64::from(self.internal_config().confirmations_delta)
        }
    }

    pub(crate) fn extra_msg_relayer_delta(&self, relayer_account_id: &AccountId) -> u64 {
        if self
            .data()
            .extra_msg_relayer_white_list
            .contains(relayer_account_id)
        {
            0
        } else {
            u64::from(self.internal_config().extra_msg_confirmations_delta)
        }
    }

    pub(crate) fn confirmations_delta_for(
        &self,
        relayer_account_id: Option<&AccountId>,
        has_extra_msg: bool,
    ) -> u64 {
        match relayer_account_id {
            Some(relayer_account_id) if has_extra_msg => {
                self.extra_msg_relayer_delta(relayer_account_id)
            }
            Some(relayer_account_id) => self.relayer_delta(relayer_account_id),
            None if has_extra_msg => {
                u64::from(self.internal_config().extra_msg_confirmations_delta)
            }
            None => u64::from(self.internal_config().confirmations_delta),
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

    #[test]
    fn test_set_confirmations_strategy_updates_config() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());

        let range_upper_bound = U128(20_000_000);
        let confirmations = 6u8;

        unit_env
            .contract
            .set_confirmations_strategy(range_upper_bound, confirmations);

        assert_eq!(
            unit_env
                .contract
                .internal_config()
                .confirmations_strategy
                .get(&range_upper_bound.0.to_string()),
            Some(&confirmations),
            "confirmations_strategy must be updated with the new entry"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid confirmations_strategy")]
    fn test_set_confirmations_strategy_over_100_panics() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());

        unit_env
            .contract
            .set_confirmations_strategy(U128(20_000_000), 101);
    }

    #[test]
    #[should_panic(expected = "confirmations_strategy must be non-decreasing")]
    fn test_set_confirmations_strategy_non_monotonic_panics() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());

        unit_env.contract.set_confirmations_strategy(U128(5_000), 3);
    }

    #[test]
    fn test_get_confirmations_max_fallback_is_max_tier() {
        let mut unit_env = init_unit_env();
        testing_env!(unit_env
            .context
            .predecessor_account_id(owner_id())
            .attached_deposit(NearToken::from_yoctonear(1))
            .build());

        unit_env
            .contract
            .set_confirmations_strategy(U128(10_000_000), 10);
        unit_env
            .contract
            .set_confirmations_strategy(U128(10_000), 3);
        unit_env
            .contract
            .set_confirmations_strategy(U128(50_000), 10);
        unit_env
            .contract
            .remove_confirmations_strategy(U128(10_000_000));

        let config = unit_env.contract.internal_config();
        assert_eq!(config.get_confirmations(u128::MAX), 10);
        assert_eq!(u64::from(config.max_tier_confirmations()), 10);
        assert_eq!(config.get_confirmations(5_000), 3);
    }

    // Regression: a Zcash unified address with no transparent receiver (shielded-only,
    // e.g. Sapling+Orchard) has no scriptPubKey. `string_to_script_pubkey` panics on it
    // ("Failed to get script pubkey: No receiver found in address"), which is what broke
    // Orchard withdrawals to such addresses. `target_script_pubkey` must return `None`
    // for it (so the withdraw path treats transparent outputs as change) while still
    // resolving transparent addresses.
    #[test]
    #[cfg(feature = "zcash")]
    fn test_target_script_pubkey_shielded_only_ua_is_none() {
        use crate::network::{Address, Chain};

        let mut unit_env = init_unit_env();
        let config = unit_env.contract.internal_mut_config();
        config.chain = Chain::ZcashMainnet;

        // Real mainnet recipient from the failed withdrawal: Sapling + Orchard, no
        // transparent receiver.
        let shielded_only_ua = "u15a97e324mckwx89t0ucxytpd7v3pfzey7daldrk4mwu3u55ej39f6v7myqjxw0e098hnhyp0tvfgfnxj8swt22rl4f77a8wrg9zjynh9dwj20lf232h7yzfr0v53l2s824l22l63xwlxyypnxkx9qq7dd249pj565q7490fey5czu2pm";

        // Precondition documenting the bug: the address parses, but yields no scriptPubKey.
        assert!(
            Address::parse(shielded_only_ua, Chain::ZcashMainnet)
                .expect("valid unified address")
                .script_pubkey()
                .is_err(),
            "fixture must be a shielded-only UA with no transparent receiver"
        );

        assert!(
            config.target_script_pubkey(shielded_only_ua).is_none(),
            "shielded-only UA must yield no transparent scriptPubKey instead of panicking"
        );

        // A transparent t1 address still resolves to a scriptPubKey.
        assert!(
            config
                .target_script_pubkey("t1KfwsnwJeNRVjQGBDZhwKskpQbih2qx5Ua")
                .is_some(),
            "transparent address must resolve to a scriptPubKey"
        );
    }
}
