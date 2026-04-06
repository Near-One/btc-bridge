#[cfg(not(feature = "zcash"))]
use crate::network::Chain::BitcoinTestnet;
#[cfg(feature = "zcash")]
use crate::network::Chain::ZcashTestnet;
use crate::*;
use near_sdk::test_utils::VMContextBuilder;
pub use near_sdk::testing_env;

mod post_action;
mod storage;
mod utils;

pub fn burrowland_id() -> AccountId {
    "burrowland_id".parse().unwrap()
}

pub fn user_id() -> AccountId {
    "user_id".parse().unwrap()
}

pub fn owner_id() -> AccountId {
    "owner_id".parse().unwrap()
}

pub fn recipient_id() -> AccountId {
    "recipient_id".parse().unwrap()
}

pub fn chain_signatures_id() -> AccountId {
    "chain_signatures_id".parse().unwrap()
}

pub fn nbtc_id() -> AccountId {
    "nbtc_id".parse().unwrap()
}

pub fn btc_light_client_id() -> AccountId {
    "btc_light_client_id".parse().unwrap()
}

pub fn init_contract() -> Contract {
    Contract::new(Config {
        #[cfg(not(feature = "zcash"))]
        chain: BitcoinTestnet,
        #[cfg(feature = "zcash")]
        chain: ZcashTestnet,
        chain_signatures_account_id: chain_signatures_id(),
        nbtc_account_id: nbtc_id(),
        btc_light_client_account_id: btc_light_client_id(),
        confirmations_strategy: HashMap::from([("10000000".to_string(), 2)]),
        confirmations_delta: 1,
        extra_msg_confirmations_delta: 1,
        withdraw_bridge_fee: BridgeFee {
            fee_min: 50000,
            fee_rate: 0,
            protocol_fee_rate: 9000,
        },
        deposit_bridge_fee: BridgeFee {
            fee_min: 0,
            fee_rate: 0,
            protocol_fee_rate: 9000,
        },
        min_deposit_amount: 20000,
        min_withdraw_amount: 70000,
        min_change_amount: 0,
        max_change_amount: u128::MAX,
        min_btc_gas_fee: 10000,
        max_btc_gas_fee: 50000,
        max_withdrawal_input_number: 10,
        max_change_number: 10,
        max_active_utxo_management_input_number: 2,
        max_active_utxo_management_output_number: 2,
        active_management_lower_limit: 0,
        active_management_upper_limit: u32::MAX,
        passive_management_lower_limit: 0,
        passive_management_upper_limit: u32::MAX,
        rbf_num_limit: 99,
        max_btc_tx_pending_sec: 3600 * 24,
        chain_signatures_root_public_key: None,
        change_address: None,
        unhealthy_utxo_amount: 1000,
        #[cfg(feature = "zcash")]
        expiry_height_gap: 1000,
        refund_timelock_sec: 3600,
    })
}

pub struct UnitEnv {
    pub contract: Contract,
    pub context: VMContextBuilder,
}

pub fn init_unit_env() -> UnitEnv {
    let mut context = VMContextBuilder::new();
    testing_env!(context.predecessor_account_id(owner_id()).build());
    let contract = init_contract();
    UnitEnv { contract, context }
}
