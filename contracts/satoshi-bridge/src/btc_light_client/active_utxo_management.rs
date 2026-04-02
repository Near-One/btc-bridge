use super::assert_verification_succeeded;
use crate::{env, near, BTCPendingInfo, Contract, ContractExt, Gas, Promise, PromiseOrValue};

pub const GAS_FOR_VERIFY_ACTIVE_UTXO_MANAGEMENT_CALL_BACK: Gas = Gas::from_tgas(50);

impl Contract {
    pub fn internal_verify_active_utxo_management(
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

        #[cfg(feature = "dash")]
        let verify_promise = {
            let _ = (&tx_block_blockhash, tx_index, &merkle_proof);
            self.verify_transaction_via_mpc(tx_id.clone(), confirmations)
        };

        verify_promise.then(
            Self::ext(env::current_account_id())
                .with_static_gas(GAS_FOR_VERIFY_ACTIVE_UTXO_MANAGEMENT_CALL_BACK)
                .verify_active_utxo_management_callback(tx_id),
        )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn verify_active_utxo_management_callback(
        &mut self,
        tx_id: String,
    ) -> PromiseOrValue<bool> {
        assert_verification_succeeded();
        self.internal_unwrap_btc_pending_info(&tx_id)
            .assert_pending_verify();
        self.internal_unwrap_mut_btc_pending_info(&tx_id)
            .to_pending_burn_stage();
        self.verify_active_utxo_management_burn_promise(tx_id)
            .into()
    }
}
