use crate::{json, log, AccountId, DepositMsg, SignatureResponse, U128};
use near_sdk::serde::Serialize;

const EVENT_STANDARD: &str = "bridge";
const EVENT_STANDARD_VERSION: &str = "1.0.0";

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum Event<'a> {
    DepositProtocolFee {
        account_id: &'a AccountId,
        amount: U128,
    },
    VerifyDepositDetails {
        recipient_id: &'a AccountId,
        mint_amount: U128,
        protocol_fee: U128,
        relayer_account_id: AccountId,
        relayer_fee: U128,
        success: bool,
    },
    VerifyWithdrawDetails {
        account_id: &'a AccountId,
        burn_amount: U128,
        protocol_fee: U128,
        relayer_account_id: AccountId,
        relayer_fee: U128,
        refund: U128,
        success: bool,
    },
    VerifyActiveUtxoManagementDetails {
        account_id: &'a AccountId,
        burn_amount: U128,
        success: bool,
    },
    UnavailableUtxo {
        recipient_id: &'a AccountId,
        utxo_storage_key: &'a String,
        amount: U128,
    },
    GenerateBtcPendingInfo {
        account_id: &'a AccountId,
        btc_pending_id: &'a String,
    },
    BtcInputSignature {
        account_id: &'a AccountId,
        btc_pending_id: &'a String,
        sign_index: usize,
        signature: &'a SignatureResponse,
    },
    SignedBtcTransaction {
        account_id: &'a AccountId,
        tx_id: String,
        #[cfg(not(feature = "zcash"))]
        tx_bytes: &'a Vec<u8>,
        #[cfg(feature = "zcash")]
        tx_bytes_base64: String,
    },
    WithdrawBtcDetail {
        cost_nbtc: U128,
        withdraw_fee: U128,
        btc_gas_fee: U128,
        actual_received_amount: U128,
    },
    WithdrawBridgeProtocolFee {
        amount: U128,
        success: bool,
    },
    LogDepositAddress {
        deposit_msg: DepositMsg,
        path: String,
        deposit_address: String,
    },
    TransferNbtc {
        account_id: &'a AccountId,
        amount: U128,
        success: bool,
    },
    LostFoundNbtc {
        account_id: &'a AccountId,
        amount: U128,
    },
    UtxoAdded {
        utxo_storage_keys: Vec<String>,
        balances: Option<Vec<U128>>,
    },
    UtxoRemoved {
        utxo_storage_keys: Vec<String>,
    },
    InvalidPostAction {
        index: Option<usize>,
        err_msg: String,
    },
    RefundRequested {
        deposit_msg: DepositMsg,
        utxo_storage_key: String,
        amount: U128,
        refund_address: String,
        gas_fee: U128,
    },
    RefundRejected {
        utxo_storage_key: String,
    },
    RefundExecuted {
        utxo_storage_key: String,
        amount: U128,
        refund_address: String,
    },
    TokenMigrated {
        new_token: &'a AccountId,
        accounts: usize,
        total_amount: U128,
    },
    TokenMigrationRolledBack {
        new_token: &'a AccountId,
        accounts: usize,
        total_amount: U128,
    },
}

impl Event<'_> {
    pub fn emit(&self) {
        emit_event(&self);
    }
}

pub(crate) fn emit_event<T: ?Sized + Serialize>(data: &T) {
    let result = json!(data);
    let event_json = json!({
        "standard": EVENT_STANDARD,
        "version": EVENT_STANDARD_VERSION,
        "event": result["event"],
        "data": [result["data"]]
    })
    .to_string();
    log!("EVENT_JSON:{}", event_json);
}
