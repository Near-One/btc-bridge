use crate::{
    env, legacy::ConfigV2, near, Config, Contract, ContractExt, StorageKey, VersionedContractData,
};
use near_sdk::borsh::{self, BorshDeserialize};
use near_sdk::IntoStorageKey;

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
            VersionedContractData::Current(data) => VersionedContractData::Current(data),
        };
        contract
    }

    /// Migrates only the stored `Config` bytes from the previous schema (`ConfigV2`)
    /// to the current `Config`, without touching the rest of the contract state.
    ///
    /// Use when the versioned state is already at `Current` but the on-storage
    /// `Config` still has the old byte layout (e.g. a new field was added to
    /// `Config` and a previous migration did not rewrite it).
    #[private]
    pub fn migrate_config(&mut self) {
        let storage_key = StorageKey::Config.into_storage_key();
        let bytes = env::storage_read(&storage_key).expect("ERR_CONFIG: not found in storage");
        let old_config = ConfigV2::try_from_slice(&bytes)
            .expect("ERR_CONFIG: failed to deserialize as ConfigV2");
        let new_config: Config = old_config.into();
        env::storage_write(
            &storage_key,
            &borsh::to_vec(&new_config).expect("ERR_CONFIG: failed to serialize Config"),
        );
    }

    /// Returns semver of this contract.
    pub fn get_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}
