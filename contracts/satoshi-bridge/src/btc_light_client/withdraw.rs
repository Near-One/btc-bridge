use super::assert_verification_succeeded;
use crate::{env, near, BTCPendingInfo, Contract, ContractExt, Gas, Promise, PromiseOrValue};

pub const GAS_FOR_VERIFY_WITHDRAW_CALL_BACK: Gas = Gas::from_tgas(50);
pub const GAS_FOR_VERIFY_CANCEL_WITHDRAW_CALL_BACK: Gas = Gas::from_tgas(50);

impl Contract {
    #[allow(unused_variables)]
    pub fn internal_verify_withdraw(
        &self,
        tx_id: String,
        tx_block_blockhash: String,
        tx_index: u64,
        merkle_proof: Vec<String>,
        btc_pending_info: &BTCPendingInfo,
    ) -> Promise {
        let config = self.internal_config();
        let confirmations = self.get_confirmations(config, btc_pending_info.actual_received_amount);

        #[cfg(not(feature = "dash"))]
        let verify_promise = self.verify_transaction_inclusion_promise(
            config.btc_light_client_account_id.clone(),
            tx_id.clone(),
            tx_block_blockhash,
            tx_index,
            merkle_proof,
            confirmations,
        );

        // For DASH (legacy P2PKH), the on-chain txid differs from the unsigned pending ID
        // because script_sig is included in the txid hash. Compute the actual signed txid
        // from tx_bytes_with_sign for the MPC verification query.
        #[cfg(feature = "dash")]
        let verify_promise = {
            let signed_tx_id = btc_pending_info.get_signed_tx_id();
            self.verify_transaction_via_mpc(signed_tx_id, confirmations)
        };

        verify_promise.then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_VERIFY_CANCEL_WITHDRAW_CALL_BACK)
                .internal_verify_withdraw_callback(tx_id),
        )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn internal_verify_withdraw_callback(&mut self, tx_id: String) -> PromiseOrValue<bool> {
        assert_verification_succeeded();
        self.internal_unwrap_btc_pending_info(&tx_id)
            .assert_pending_verify();
        self.internal_unwrap_mut_btc_pending_info(&tx_id)
            .to_pending_burn_stage();
        self.verify_withdraw_burn_promise(tx_id).into()
    }
}
