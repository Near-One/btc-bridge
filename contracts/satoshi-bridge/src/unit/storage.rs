use crate::*;

impl Contract {
    pub(crate) fn calculate_required_balance_for_safe_deposit(&mut self) -> NearToken {
        let storage_usage = env::storage_usage();

        let txid = "83d28cfcff0d86035e2742ecde99ef4801bb9f928d7d0118a0c1dd87bdc299ac".to_owned();
        let vout = u32::MAX;
        let path = get_deposit_path(&DepositMsg {
            recipient_id: env::current_account_id(),
            post_actions: None,
            extra_msg: None,
            safe_deposit: None,
            refund_address: None,
        });
        let utxo_storage_key = generate_utxo_storage_key(txid, vout);
        let tx_bytes = vec![0u8; 300]; // TODO: optimise storage usage

        self.data_mut().utxos.insert(
            utxo_storage_key.clone(),
            VUTXO::Current(UTXO {
                path,
                tx_bytes: near_sdk::json_types::Base64VecU8(tx_bytes),
                vout: vout.try_into().unwrap(),
                balance: u64::MAX,
            }),
        );

        self.data_mut()
            .verified_deposit_utxo
            .insert(utxo_storage_key);

        let required_storage_for_deposit = env::storage_usage().saturating_sub(storage_usage);

        env::storage_byte_cost().saturating_mul(required_storage_for_deposit.into())
    }
}

#[test]
fn test_storage_for_deposit() {
    let mut unit_env = init_unit_env();

    assert_eq!(
        unit_env
            .contract
            .calculate_required_balance_for_safe_deposit(),
        unit_env.contract.required_balance_for_safe_deposit()
    );
}
