#[cfg(feature = "contract")]
mod contract;

pub const WITHDRAW_MEMO_PREFIX: &str = "WITHDRAW_TO:";

pub fn withdraw_to(address: impl AsRef<str>) -> String {
    format!("{WITHDRAW_MEMO_PREFIX}{}", address.as_ref())
}
