use near_sdk::require;

use crate::DepositMsg;

pub fn assert_post_actions_supported(deposit_msg: &DepositMsg) {
    require!(
        deposit_msg.post_actions.is_none(),
        "post_actions are not supported"
    );
}

pub fn is_safe_deposit(_deposit_msg: &DepositMsg) -> bool {
    true
}
