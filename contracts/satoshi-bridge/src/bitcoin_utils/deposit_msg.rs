use crate::DepositMsg;

pub fn assert_post_actions_supported(_deposit_msg: &DepositMsg) {}

pub fn is_safe_deposit(deposit_msg: &DepositMsg) -> bool {
    deposit_msg.safe_deposit.is_some()
}
