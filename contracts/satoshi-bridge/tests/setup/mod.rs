#![allow(dead_code)]
#![allow(unused_imports)]
pub mod context;
#[cfg(feature = "zcash")]
pub mod orchard;
pub mod utils;
pub use context::*;
#[cfg(feature = "zcash")]
pub use orchard::*;
pub use utils::*;

// Re-export types used by tests
pub use bitcoin::OutPoint;
#[cfg(feature = "zcash")]
pub use satoshi_bridge::zcash_utils::types::ChainSpecificData;
pub use satoshi_bridge::{DepositMsg, TokenReceiverMessage};

/// Extension trait to convert `near_workspaces::Account` IDs (`near-account-id` v1)
/// to `near_sdk::AccountId` (`near-account-id` v2) via string roundtrip.
/// Required because `near-workspaces` and `near-sdk` depend on different major
/// versions of the `near-account-id` crate.
pub trait ToSdkAccountId {
    fn sdk_id(&self) -> near_sdk::AccountId;
}

impl ToSdkAccountId for near_workspaces::Account {
    fn sdk_id(&self) -> near_sdk::AccountId {
        self.id().as_str().parse().unwrap()
    }
}
