use crate::*;

impl Contract {
    pub fn check_withdraw_psbt_valid(
        &self,
        target_address_script_pubkey: &ScriptBuf,
        withdraw_change_address_script_pubkey: &ScriptBuf,
        withdraw_psbt: &Psbt,
        vutxos: &[VUTXO],
        amount: u128,
        withdraw_fee: u128,
        max_gas_fee: Option<U128>,
    ) -> (u128, u128) {
        let config = self.internal_config();
        let utxo_num = self.data().utxos.len() + vutxos.len() as u32;
        let (input_num, change_num, actual_received_amount, gas_fee) = self.check_withdraw_psbt(
            withdraw_psbt,
            target_address_script_pubkey,
            withdraw_change_address_script_pubkey,
            vutxos,
            amount,
            withdraw_fee,
        );

        if let Some(max_gas_fee) = max_gas_fee {
            require!(
                gas_fee <= max_gas_fee.0,
                format!(
                    "Gas fee does not match the provided max fee (gas fee = {}; max gas fee = {})",
                    gas_fee, max_gas_fee.0
                )
            );
        }

        require!(
            change_num <= config.max_change_number as usize,
            format!("change_num must not exceed {}", config.max_change_number)
        );
        require!(
            input_num <= config.max_withdrawal_input_number as usize,
            format!(
                "input must not exceed {}",
                config.max_withdrawal_input_number
            )
        );

        if utxo_num < config.passive_management_lower_limit {
            require!(input_num < change_num, "require input_num < change_num");
        } else if utxo_num > config.passive_management_upper_limit {
            require!(input_num > change_num, "require input_num > change_num");
        }

        Event::WithdrawBtcDetail {
            cost_nbtc: amount.into(),
            withdraw_fee: withdraw_fee.into(),
            btc_gas_fee: gas_fee.into(),
            actual_received_amount: actual_received_amount.into(),
        }
        .emit();
        (actual_received_amount, gas_fee)
    }

    pub fn check_active_management_psbt_valid(
        &self,
        psbt: &Psbt,
        vutxos: &[VUTXO],
    ) -> (u128, u128) {
        let config = self.internal_config();
        let utxo_num = self.data().utxos.len() + vutxos.len() as u32;
        let input_num = psbt.unsigned_tx.input.len();
        let output_num = psbt.unsigned_tx.output.len();

        if !is_merge_unhealthy_utxos(output_num, vutxos, config.unhealthy_utxo_amount) {
            if utxo_num < config.active_management_lower_limit {
                require!(input_num < output_num, "require input_num < output_num");
                require!(
                    output_num <= config.max_active_utxo_management_output_number as usize,
                    format!(
                        "require output_num <= {}",
                        config.max_active_utxo_management_output_number
                    )
                );
            } else if utxo_num > config.active_management_upper_limit {
                require!(input_num > output_num, "require input_num > output_num");
                require!(
                    input_num <= config.max_active_utxo_management_input_number as usize,
                    format!(
                        "require input_num <= {}",
                        config.max_active_utxo_management_input_number
                    )
                );
            } else {
                env::panic_str("Active management conditions are not met");
            }
        }

        let (output_amount, gas_fee) =
            self.check_psbt_output_all_change_address(psbt, vutxos, true, false);
        (output_amount, gas_fee)
    }

    pub fn check_psbt_output_all_change_address(
        &self,
        psbt: &Psbt,
        vutxos: &[VUTXO],
        force_healthy_output: bool,
        is_cancel: bool,
    ) -> (u128, u128) {
        let config = self.internal_config();
        let withdraw_change_address_script_pubkey = config.get_change_address().script_pubkey();
        let input_amount = vutxos
            .iter()
            .map(|vutxo| vutxo.get_amount() as u128)
            .sum::<u128>();
        let output_amount = psbt
            .unsigned_tx
            .output
            .iter()
            .map(|v| {
                if force_healthy_output {
                    require!(
                        v.value.to_sat() > config.unhealthy_utxo_amount
                            && v.value.to_sat() as u128 <= config.max_change_amount,
                        "The output amount is not in the valid range"
                    );
                } else {
                    require!(
                        v.value.to_sat() as u128 >= config.min_change_amount
                            && v.value.to_sat() as u128 <= config.max_change_amount,
                        "The output amount is not in the valid range"
                    );
                }
                require!(
                    v.script_pubkey == withdraw_change_address_script_pubkey,
                    "Invalid output script_pubkey"
                );
                v.value.to_sat() as u128
            })
            .sum::<u128>();
        let gas_fee = input_amount - output_amount;
        if !is_cancel {
            require!(
                gas_fee >= config.min_btc_gas_fee && gas_fee <= config.max_btc_gas_fee,
                format!(
                    "Invalid gas fee ({}). valid range: [{}, {}].",
                    gas_fee, config.min_btc_gas_fee, config.max_btc_gas_fee
                )
            );
        }
        (output_amount, gas_fee)
    }

    pub fn check_withdraw_psbt(
        &self,
        psbt: &Psbt,
        target_address_script_pubkey: &ScriptBuf,
        withdraw_change_address_script_pubkey: &ScriptBuf,
        vutxos: &[VUTXO],
        amount: u128,
        withdraw_fee: u128,
    ) -> (usize, usize, u128, u128) {
        let config = self.internal_config();
        let input_amounts = vutxos.iter().map(|vutxo| vutxo.get_amount() as u128);
        let min_input_amount = input_amounts.clone().min().unwrap();
        let total_input_amount = input_amounts.sum::<u128>();
        let mut total_output_amount = 0;
        let mut actual_received_amounts = vec![];
        let mut change_amounts = vec![];
        psbt.unsigned_tx.output.iter().for_each(|output| {
            let output_value = output.value.to_sat() as u128;
            total_output_amount += output_value;
            if &output.script_pubkey == target_address_script_pubkey {
                actual_received_amounts.push(output_value);
            } else if &output.script_pubkey == withdraw_change_address_script_pubkey {
                require!(
                    output_value >= config.min_change_amount,
                    "The change amount is too small"
                );
                require!(
                    output_value < min_input_amount,
                    "The change amount must be less than all inputs"
                );
                change_amounts.push(output_value);
            } else {
                let output_address = Address::from_script(&output.script_pubkey, btc_network())
                    .expect("Unsupported btc address type");
                env::panic_str(
                    format!("Invalid transaction output address: {}", output_address).as_str(),
                );
            }
        });
        require!(
            actual_received_amounts.len() == 1,
            "only one user output is allowed."
        );
        let actual_received_amount = actual_received_amounts[0];
        let input_num = psbt.unsigned_tx.input.len();
        let change_num = change_amounts.len();
        if input_num > change_num {
            require!(
                change_amounts
                    .into_iter()
                    .all(|v| v < config.max_change_amount),
                format!(
                    "Any change amount should be less than {} when input_num > change_num",
                    config.max_change_amount
                )
            );
        }
        let gas_fee = total_input_amount - total_output_amount;
        // When constructing the withdraw transaction, if the change is less than min_change_amount (dust),
        // the caller may deduct a portion from the user's output to make the change amount meet min_change_amount.
        // Therefore, the contract relaxes the validation.
        let max_received_amount = amount - withdraw_fee - gas_fee;
        let min_received_amount = max_received_amount - config.min_change_amount;
        require!(
            actual_received_amount >= min_received_amount
                && actual_received_amount <= max_received_amount,
            format!(
                "The user's output amount ({}) is out of the valid range ({}, {})",
                actual_received_amount, min_received_amount, max_received_amount
            )
        );
        require!(
            gas_fee >= config.min_btc_gas_fee && gas_fee <= config.max_btc_gas_fee,
            format!(
                "Invalid gas fee ({}). valid range: [{}, {}].",
                gas_fee, config.min_btc_gas_fee, config.max_btc_gas_fee
            )
        );
        (input_num, change_num, actual_received_amount, gas_fee)
    }
}

impl Contract {
    pub fn generate_psbt_and_vutxos(
        &mut self,
        input: Vec<OutPoint>,
        output: Vec<TxOut>,
    ) -> (Psbt, Vec<String>, Vec<VUTXO>) {
        require!(!input.is_empty(), "empty input");
        require!(!output.is_empty(), "empty output");
        let transaction = BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: input
                .into_iter()
                .map(|previous_output| TxIn {
                    previous_output,
                    sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
                    ..Default::default()
                })
                .collect(),
            output,
        };
        let mut psbt = Psbt::from_unsigned_tx(transaction).expect("Failed to generate PSBT");
        let (utxo_storage_keys, vutxos) = self.remove_vutxo_by_psbt(&psbt);
        vutxos.iter().enumerate().for_each(|(i, v)| {
            psbt.inputs[i].witness_utxo = Some(TxOut {
                value: Amount::from_sat(v.get_amount()),
                script_pubkey: self
                    .generate_btc_p2wpkh_address(&v.get_path())
                    .script_pubkey(),
            })
        });
        (psbt, utxo_storage_keys, vutxos)
    }

    pub fn generate_psbt_from_original_psbt_and_new_output(
        &self,
        original_tx_btc_pending_info: &BTCPendingInfo,
        output: Vec<TxOut>,
    ) -> Psbt {
        let original_psbt = original_tx_btc_pending_info.get_psbt();
        let transaction = BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: original_psbt
                .unsigned_tx
                .input
                .into_iter()
                .map(|original_psbt_input| TxIn {
                    previous_output: original_psbt_input.previous_output,
                    sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
                    ..Default::default()
                })
                .collect(),
            output,
        };
        let mut psbt = Psbt::from_unsigned_tx(transaction).expect("Failed to generate PSBT");
        original_psbt.inputs.iter().enumerate().for_each(|(i, v)| {
            psbt.inputs[i].witness_utxo.clone_from(&v.witness_utxo);
        });
        psbt
    }
}

pub fn is_merge_unhealthy_utxos(
    output_num: usize,
    vutxos: &[VUTXO],
    unhealthy_utxo_amount: u64,
) -> bool {
    output_num == 1
        && vutxos
            .iter()
            .all(|v| v.get_amount() <= unhealthy_utxo_amount)
}
