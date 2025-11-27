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
            } => self.ft_on_transfer_withdraw_chain_specific(
                sender_id,
                amount,
                target_btc_address,
                input,
                output,
                max_gas_fee,
                orchard_bundle_bytes.map(|b| hex::decode(b).unwrap()),
            ),
        }
    }
}

impl Contract {
    /// Validate transparent change outputs for Orchard withdrawals.
    /// For Orchard withdrawals, transparent outputs must only be change back to the bridge.
    /// Validates exact accounting: input = orchard_amount + change + gas_fee
    fn check_orchard_with_transparent_change(
        &self,
        psbt: &PsbtWrapper,
        vutxos: &[VUTXO],
        withdraw_change_address_script_pubkey: &ScriptBuf,
        orchard_amount: u128,
        gas_fee: u128,
    ) {
        let config = self.internal_config();
        let input_amount = vutxos
            .iter()
            .map(|vutxo| vutxo.get_amount() as u128)
            .sum::<u128>();

        let mut total_change_amount = 0u128;

        // All transparent outputs must be change outputs to bridge address
        for output in psbt.get_output() {
            let output_value = output.value.to_sat() as u128;
            require!(
                &output.script_pubkey == withdraw_change_address_script_pubkey,
                "For Orchard withdrawals, all transparent outputs must be change to bridge address"
            );
            require!(
                output_value >= config.min_change_amount,
                "Change amount is too small"
            );
            require!(
                output_value <= config.max_change_amount,
                "Change amount exceeds maximum"
            );
            total_change_amount += output_value;
        }

        // Validate exact accounting: transparent_input = orchard_amount + change + gas_fee
        // This ensures the value balance is correct and no value is missing or extra
        let expected_input = orchard_amount + total_change_amount + gas_fee;
        require!(
            input_amount == expected_input,
            format!(
                "Transparent accounting mismatch: input ({}) != orchard ({}) + change ({}) + fee ({})",
                input_amount, orchard_amount, total_change_amount, gas_fee
            )
        );
    }

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

        let withdraw_fee = self.internal_config().withdraw_bridge_fee.get_fee(amount);

        // Calculate actual gas fee using ZIP-317 formula
        let computed_gas_fee = psbt.get_min_fee().into_u64() as u128;

        // Determine validation path based on presence of Orchard bundle
        let (actual_received_amount, gas_fee) = if psbt.has_orchard_bundle() {
            // Orchard withdrawal case (with or without transparent change)
            // Baseline scenario: 1 Orchard output to user + transparent change to bridge

            // Use max_gas_fee as upper bound if provided, otherwise use computed fee
            let gas_fee = if let Some(max_fee) = max_gas_fee {
                std::cmp::min(max_fee.0, computed_gas_fee)
            } else {
                computed_gas_fee
            };

            // Recover and validate the actual Orchard output amount from the bundle
            let actual_orchard_amount = psbt.get_orchard_output_amount();
            let expected_max = amount.saturating_sub(withdraw_fee).saturating_sub(gas_fee);
            let expected_min = expected_max.saturating_sub(self.internal_config().min_change_amount);

            require!(
                actual_orchard_amount >= expected_min && actual_orchard_amount <= expected_max,
                format!(
                    "Orchard output amount ({}) out of valid range ({}, {})",
                    actual_orchard_amount, expected_min, expected_max
                )
            );

            // If there are transparent outputs, validate they are only change outputs
            if psbt.get_output_num() > 0 {
                let withdraw_change_address_script_pubkey =
                    self.internal_config().get_change_script_pubkey();

                // Validate all transparent outputs are valid change with exact accounting
                self.check_orchard_with_transparent_change(
                    psbt,
                    &vutxos,
                    &withdraw_change_address_script_pubkey,
                    actual_orchard_amount,
                    gas_fee,
                );
            }

            (actual_orchard_amount, gas_fee)
        } else {
            // Pure transparent withdrawal case: validate transparent outputs
            let target_address_script_pubkey = self
                .internal_config()
                .string_to_script_pubkey(&target_btc_address);

            let withdraw_change_address_script_pubkey =
                self.internal_config().get_change_script_pubkey();

            self.check_withdraw_psbt_valid(
                &target_address_script_pubkey,
                &withdraw_change_address_script_pubkey,
                psbt,
                &vutxos,
                amount,
                withdraw_fee,
                max_gas_fee,
            )
        };

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
