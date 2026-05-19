use crate::{
    near, require, AccessControllable, Contract, ContractExt, Pausable, PromiseOrValue, Role,
};

use near_plugins::pause;

#[near]
impl Contract {
    /// Sign the specified input of the BTC transaction, and when signing the last unsigned input, generate a signed transaction ready to be broadcasted.
    ///
    /// # Arguments
    ///
    /// * `btc_pending_sign_id` - Pending signature BTC transaction ID.
    /// * `sign_index` -  Specify the input index for this signature.
    ///
    /// # Returns
    ///
    /// bool - Whether the signature was successful.
    #[payable]
    #[pause(except(roles(Role::DAO)))]
    pub fn sign_btc_transaction(
        &mut self,
        btc_pending_sign_id: String,
        sign_index: usize,
        key_version: u32,
    ) -> PromiseOrValue<bool> {
        let btc_pending_info = self.internal_unwrap_btc_pending_info(&btc_pending_sign_id);
        btc_pending_info.assert_pending_sign();
        if let Some(original_tx_id) = btc_pending_info.get_original_tx_id() {
            if !self.check_btc_pending_info_exists(original_tx_id) {
                require!(
                    self.internal_unwrap_mut_account(&btc_pending_info.account_id.clone())
                        .btc_pending_sign_ids
                        .remove(&btc_pending_sign_id),
                    "Internal error"
                );
                self.internal_remove_btc_pending_info(&btc_pending_sign_id);
                return PromiseOrValue::Value(true);
            }
        }
        self.internal_sign_btc_transaction(btc_pending_sign_id, sign_index, key_version)
            .into()
    }
}
