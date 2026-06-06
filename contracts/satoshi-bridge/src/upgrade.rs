use crate::{
    env, legacy::migrate_btc_pending_infos_to_current, near, Contract, ContractExt,
    VersionedContractData,
};

#[near]
impl Contract {
    /// A method to migrate a state during the contract upgrade.
    /// Can only be called after upgrade method.
    #[private]
    #[init(ignore_state)]
    pub fn migrate_state() -> Self {
        let mut contract: Contract = env::state_read().expect("NOT INIT");
        contract.data = match contract.data {
            VersionedContractData::V0(data) => VersionedContractData::Current(data.into()),
            VersionedContractData::V1(data) => VersionedContractData::Current(data.into()),
            VersionedContractData::V2(data) => VersionedContractData::Current(data.into()),
            VersionedContractData::V3(data) => VersionedContractData::Current(data.into()),
            VersionedContractData::V4(data) => VersionedContractData::Current(data.into()),
            VersionedContractData::Current(mut data) => {
                // Ensure all `VBTCPendingInfo` entries are in the `Current` variant
                // even when the outer `ContractData` schema did not change. Without
                // this, an upgrade that only modifies the inner pending-info schema
                // leaves entries in the older variant, and `internal_unwrap_*`
                // paths that take `&BTCPendingInfo` hit `unreachable!()`.
                migrate_btc_pending_infos_to_current(&mut data.btc_pending_infos);
                VersionedContractData::Current(data)
            }
        };
        contract
    }

    /// Returns semver of this contract.
    pub fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}
