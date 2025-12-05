use crate::psbt_wrapper::PsbtWrapper;
use crate::*;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_plugins::pause;

pub const GAS_FOR_FT_ON_TRANSFER_CALL_BACK: Gas = Gas::from_tgas(100);

#[near(serializers = [json])]
pub enum TokenReceiverMessage {
    DepositProtocolFee,
    // Here is the withdraw message structure that will be sent from user or dApp to the btc/zcash connector
    Withdraw {
        target_btc_address: String,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
        max_gas_fee: Option<U128>,
        orchard_bundle_bytes: Option<String>,
        expiry_height: Option<u32>,
    },
}

#[near]
impl FungibleTokenReceiver for Contract {
    #[pause(except(roles(Role::DAO)))]
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let amount = amount.into();
        require!(
            amount >= self.internal_config().min_withdraw_amount,
            "Invalid amount"
        );
        let message = serde_json::from_str::<TokenReceiverMessage>(&msg).expect("INVALID MSG");
        let token_id = env::predecessor_account_id();
        require!(
            token_id == self.internal_config().nbtc_account_id,
            "Invalid token_id"
        );
        match message {
            TokenReceiverMessage::DepositProtocolFee => {
                self.data_mut().acc_collected_protocol_fee += amount;
                self.data_mut().cur_available_protocol_fee += amount;
                Event::DepositProtocolFee {
                    account_id: &sender_id,
                    amount: U128(amount),
                }
                .emit();
                PromiseOrValue::Value(U128(0))
            }
            TokenReceiverMessage::Withdraw {
                target_btc_address,
                input,
                output,
                max_gas_fee,
                orchard_bundle_bytes,
                expiry_height,
            } => self.ft_on_transfer_withdraw_chain_specific(
                sender_id,
                amount,
                target_btc_address,
                input,
                output,
                max_gas_fee,
                orchard_bundle_bytes.map(|b| hex::decode(b).unwrap()),
                expiry_height,
            ),
        }
    }
}

impl Contract {
    pub(crate) fn create_btc_pending_info(
        &mut self,
        sender_id: AccountId,
        amount: u128,
        target_btc_address: String,
        psbt: &mut PsbtWrapper,
        max_gas_fee: Option<U128>,
    ) {
        let (utxo_storage_keys, vutxos) = self.generate_vutxos(psbt);
        require!(
            self.internal_unwrap_or_create_mut_account(&sender_id)
                .btc_pending_sign_id
                .is_none(),
            "Previous btc tx has not been signed"
        );

        let withdraw_change_address_script_pubkey =
            self.internal_config().get_change_script_pubkey();
        let withdraw_fee = self.internal_config().withdraw_bridge_fee.get_fee(amount);
        let (actual_received_amount, gas_fee) = self.check_withdraw_psbt_valid(
            target_btc_address,
            &withdraw_change_address_script_pubkey,
            psbt,
            &vutxos,
            amount,
            withdraw_fee,
            max_gas_fee,
        );

        let need_signature_num = psbt.get_input_num();
        let psbt_hex = psbt.serialize();
        let btc_pending_id = psbt.get_pending_id();
        let btc_pending_info = BTCPendingInfo {
            account_id: sender_id.clone(),
            btc_pending_id: btc_pending_id.clone(),
            transfer_amount: amount,
            actual_received_amount,
            withdraw_fee,
            gas_fee,
            burn_amount: actual_received_amount + gas_fee,
            psbt_hex,
            vutxos,
            signatures: vec![None; need_signature_num],
            tx_bytes_with_sign: None,
            create_time_sec: nano_to_sec(env::block_timestamp()),
            last_sign_time_sec: 0,
            state: PendingInfoState::WithdrawOriginal(OriginalState {
                stage: PendingInfoStage::PendingSign,
                max_gas_fee: gas_fee,
                last_rbf_time_sec: None,
                cancel_rbf_reserved: None,
            }),
        };
        require!(
            self.data_mut()
                .btc_pending_infos
                .insert(btc_pending_id.clone(), btc_pending_info.into())
                .is_none(),
            "pending info already exist"
        );
        self.internal_unwrap_mut_account(&sender_id)
            .btc_pending_sign_id = Some(btc_pending_id.clone());
        Event::UtxoRemoved { utxo_storage_keys }.emit();
        Event::GenerateBtcPendingInfo {
            account_id: &sender_id,
            btc_pending_id: &btc_pending_id,
        }
        .emit();
    }
}
