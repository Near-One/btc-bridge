use crate::{
    env, ext_nbtc, is_promise_success, near, Account, AccountId, Contract, ContractExt, Event, Gas,
    PendingUTXOInfo, PostAction, Promise, U128,
};

pub const GAS_FOR_MINT_CALL: Gas = Gas::from_tgas(150);
pub const GAS_FOR_MINT_CALL_BACK: Gas = Gas::from_tgas(10);

impl Contract {
    pub fn internal_mint_promise(
        &self,
        recipient_id: AccountId,
        mint_amount: U128,
        protocol_fee: U128,
        relayer_fee: U128,
        pending_utxo_info: PendingUTXOInfo,
        post_actions: Option<Vec<PostAction>>,
    ) -> Promise {
        ext_nbtc::ext(self.internal_config().nbtc_account_id.clone())
            .with_static_gas(GAS_FOR_MINT_CALL)
            .mint(
                recipient_id.clone(),
                mint_amount,
                protocol_fee,
                env::signer_account_id(),
                relayer_fee,
                post_actions,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_MINT_CALL_BACK)
                    .mint_callback(
                        recipient_id.clone(),
                        mint_amount,
                        protocol_fee,
                        relayer_fee,
                        pending_utxo_info,
                    ),
            )
    }
}

#[near]
impl Contract {
    #[private]
    pub fn mint_callback(
        &mut self,
        recipient_id: AccountId,
        mint_amount: U128,
        protocol_fee: U128,
        relayer_fee: U128,
        pending_utxo_info: PendingUTXOInfo,
    ) -> bool {
        let is_success = is_promise_success();
        if is_success {
            if !self.check_account_exists(&recipient_id) {
                self.internal_set_account(&recipient_id, Account::new(&recipient_id));
            }
            if protocol_fee.0 > 0 {
                self.data_mut().acc_collected_protocol_fee += protocol_fee.0;
                self.data_mut().cur_available_protocol_fee += protocol_fee.0;
            }
            Event::UtxoAdded {
                utxo_storage_keys: vec![pending_utxo_info.utxo_storage_key.clone()],
                balances: Some(vec![U128::from(<u64 as Into<u128>>::into(
                    pending_utxo_info.utxo.balance,
                ))]),
            }
            .emit();
            self.internal_set_utxo(&pending_utxo_info.utxo_storage_key, pending_utxo_info.utxo);
        } else {
            self.data_mut()
                .verified_deposit_utxo
                .remove(&pending_utxo_info.utxo_storage_key);
        }
        Event::VerifyDepositDetails {
            recipient_id: &recipient_id,
            mint_amount,
            protocol_fee,
            relayer_account_id: env::signer_account_id(),
            relayer_fee,
            success: is_success,
        }
        .emit();
        is_success
    }
}
