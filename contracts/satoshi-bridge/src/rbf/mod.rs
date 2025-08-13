use crate::psbt_wrapper::PsbtWrapper;
use crate::*;

pub mod active_utxo_management;
pub mod cancel_active_utxo_management;
pub mod cancel_withdraw;
pub mod withdraw;

impl Contract {
    pub fn set_rbf_pending_info(
        &mut self,
        original_btc_pending_verify_id: &str,
        mut btc_pending_info: BTCPendingInfo,
        psbt: PsbtWrapper,
        is_cancel: bool,
    ) -> String {
        let rbf_psbt_hex = psbt.serialize();
        let btc_pending_id = psbt.get_pending_id(&self.internal_config().chain);
        btc_pending_info.btc_pending_id.clone_from(&btc_pending_id);
        btc_pending_info.psbt_hex = rbf_psbt_hex;
        require!(
            self.data_mut()
                .btc_pending_infos
                .insert(btc_pending_id.clone(), btc_pending_info.into())
                .is_none(),
            "pending info already exist"
        );
        let rbf_txs = self
            .data_mut()
            .rbf_txs
            .entry(original_btc_pending_verify_id.to_owned())
            .or_default();
        require!(rbf_txs.insert(btc_pending_id.clone()), "Rbf already exist");
        if !is_cancel {
            require!(
                rbf_txs.len() <= self.internal_config().rbf_num_limit.into(),
                "Exceed rbf_num_limit"
            );
        }
        btc_pending_id
    }
}

pub fn init_rbf_btc_pending_info(
    original_tx_btc_pending_info: &BTCPendingInfo,
    state: PendingInfoState,
) -> BTCPendingInfo {
    BTCPendingInfo {
        account_id: original_tx_btc_pending_info.account_id.clone(),
        btc_pending_id: Default::default(),
        transfer_amount: original_tx_btc_pending_info.transfer_amount,
        actual_received_amount: original_tx_btc_pending_info.actual_received_amount,
        withdraw_fee: original_tx_btc_pending_info.withdraw_fee,
        burn_amount: original_tx_btc_pending_info.burn_amount,
        gas_fee: original_tx_btc_pending_info.gas_fee,
        psbt_hex: Default::default(),
        vutxos: original_tx_btc_pending_info.vutxos.clone(),
        signatures: vec![None; original_tx_btc_pending_info.signatures.len()],
        tx_bytes_with_sign: None,
        create_time_sec: nano_to_sec(env::block_timestamp()),
        last_sign_time_sec: 0,
        state,
    }
}
