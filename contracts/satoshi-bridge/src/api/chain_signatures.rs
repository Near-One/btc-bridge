use crate::{near, AccessControllable, Contract, ContractExt, Pausable, PromiseOrValue, Role};

use near_plugins::pause;

#[near]
impl Contract {
    /// Sign the specified input of the BTC transaction, and when signing the last unsigned input, generate a signed transaction ready to be broadcasted.
    ///
    /// Requires an active trusted relayer, except for refund transactions,
    /// which anyone can sign so users can drive the refund flow themselves.
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
        if !self.internal_prepare_sign_btc_transaction(&btc_pending_sign_id) {
            return PromiseOrValue::Value(true);
        }
        self.internal_sign_btc_transaction(btc_pending_sign_id, sign_index, key_version)
            .into()
    }

    /// Sign several inputs of the BTC transaction in one call. When the last
    /// unsigned input is signed, generate a signed transaction ready to be
    /// broadcasted.
    ///
    /// Same access rules as `sign_btc_transaction`, and paused together with
    /// it. Indexes that are already signed are skipped, so a retry with the
    /// same list is safe. Attach at least 1 yoctoNEAR per input to sign; the
    /// whole deposit is split between the MPC `sign` calls, and MPC refunds
    /// any excess to this contract, as for `sign_btc_transaction`. Attach
    /// `60 + 30 * N` Tgas of prepaid gas for N inputs to sign (see
    /// `GAS_FOR_BATCH_SIGN_*`); gas left over goes to the callback and is
    /// refunded if unused. With the 1 PGas limit per transaction that allows
    /// up to 31 inputs per call.
    ///
    /// # Arguments
    ///
    /// * `btc_pending_sign_id` - Pending signature BTC transaction ID.
    /// * `sign_indexes` - Input indexes to sign. Duplicates are ignored.
    ///
    /// # Returns
    ///
    /// bool - Whether every requested signature was stored.
    #[payable]
    #[pause(name = "sign_btc_transaction", except(roles(Role::DAO)))]
    pub fn sign_btc_transaction_batch(
        &mut self,
        btc_pending_sign_id: String,
        sign_indexes: Vec<usize>,
        key_version: u32,
    ) -> PromiseOrValue<bool> {
        if !self.internal_prepare_sign_btc_transaction(&btc_pending_sign_id) {
            return PromiseOrValue::Value(true);
        }
        self.internal_sign_btc_transaction_batch(btc_pending_sign_id, sign_indexes, key_version)
            .into()
    }
}
