use crate::{Contract, VUTXO};
impl Contract {
    pub(crate) fn check_psbt_chain_specific(
        &self,
        psbt: &PsbtWrapper,
        vutxos: &[VUTXO],
        gas_fee: u128,
    ) {
    }
}
