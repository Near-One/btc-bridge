#![cfg(feature = "test-utils")]
use crate::*;

#[near]
impl Contract {
    /// Test-only: seed a UTXO directly into protocol state for a given deposit_msg.
    /// This bypasses verify_deposit flow to enable deterministic end-to-end tests
    /// on chains where constructing full tx_bytes is non-trivial.
    pub fn test_seed_utxo(
        &mut self,
        deposit_msg: DepositMsg,
        txid: String,
        vout: u32,
        amount: U128,
    ) -> String {
        let path = get_deposit_path(&deposit_msg);
        let utxo = UTXO {
            path,
            tx_bytes: vec![],
            vout: vout as usize,
            balance: amount.0 as u64,
        };
        let key = generate_utxo_storage_key(txid.clone(), vout);
        self.internal_set_utxo(&key, utxo);
        key
    }
}

