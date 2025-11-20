#![allow(dead_code)]
use std::str::FromStr;

use bitcoin::{
    absolute::LockTime, consensus::serialize, psbt::Input, transaction::Version, Address, Amount,
    Psbt, Transaction as BtcTransaction, TxIn, TxOut,
};
use near_workspaces::{result::ExecutionFinalResult, Result};

pub const PRICE_ORICE_BTC_PRICE_ID: &str = "btc_price_id";
pub const PRICE_ORICE_NEAR_PRICE_ID: &str = "near_price_id";

pub const PYTH_ORICE_BTC_PRICE_ID: &str =
    "f9c0172ba10dfa4d19088d94f5bf61d3b54d5bd7483a322a982e1373ee8ea31b";
pub const PYTH_ORICE_NEAR_PRICE_ID: &str =
    "27e867f0f4f61076456d1a73b14c7edc1cf5cef4f4d6193a33424288f11bd0f4";

pub fn generate_psbt_hex(tx_ins: Vec<(&str, u32, u64, &str)>, tx_outs: Vec<(&str, u64)>) -> String {
    let psbt = Psbt {
        unsigned_tx: BtcTransaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: tx_ins
                .iter()
                .map(|(tx_id, vout, ..)| {
                    let mut tx_in = TxIn::default();
                    tx_in.previous_output.txid = tx_id.parse().unwrap();
                    tx_in.previous_output.vout = *vout;
                    tx_in.sequence.0 = 4294967293;
                    tx_in
                })
                .collect(),
            output: tx_outs
                .iter()
                .map(|(script_addr, value)| {
                    let address = Address::from_str(script_addr)
                        .expect("Invalid btc address")
                        .assume_checked();
                    TxOut {
                        value: Amount::from_sat(*value),
                        script_pubkey: address.script_pubkey(),
                    }
                })
                .collect(),
        },
        inputs: tx_ins
            .into_iter()
            .map(|(_tx_id, _vout, amount, script_addr)| {
                let address = Address::from_str(script_addr)
                    .expect("Invalid btc address")
                    .assume_checked();
                Input {
                    witness_utxo: Some(TxOut {
                        value: Amount::from_sat(amount),
                        script_pubkey: address.script_pubkey(),
                    }),
                    ..Default::default()
                }
            })
            .collect(),
        outputs: tx_outs.iter().map(|_| Default::default()).collect(),
        version: 0,
        xpub: Default::default(),
        proprietary: Default::default(),
        unknown: Default::default(),
    };
    psbt.serialize_hex()
}

pub fn generate_tx_in(tx_id: &str, vout: u32, script_addr: Option<&str>) -> TxIn {
    let mut tx_in = TxIn::default();
    tx_in.previous_output.txid = tx_id.parse().unwrap();
    tx_in.previous_output.vout = vout;
    tx_in.sequence.0 = 4294967293;
    if let Some(script_addr) = script_addr {
        let address = Address::from_str(script_addr)
            .expect("Invalid btc address")
            .assume_checked();
        tx_in.script_sig = address.script_pubkey();
    }
    tx_in
}

pub fn generate_tx_out(value: u64, script_addr: &str) -> TxOut {
    let address = Address::from_str(script_addr)
        .expect("Invalid btc address")
        .assume_checked();
    TxOut {
        value: Amount::from_sat(value),
        script_pubkey: address.script_pubkey(),
    }
}

pub fn generate_transaction_bytes(
    tx_ins: Vec<(&str, u32, Option<&str>)>,
    tx_outs: Vec<(&str, u64)>,
) -> Vec<u8> {
    serialize(&BtcTransaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: tx_ins
            .into_iter()
            .map(|(tx_id, vout, script_addr)| generate_tx_in(tx_id, vout, script_addr))
            .collect(),
        output: tx_outs
            .into_iter()
            .map(|(script_addr, value)| generate_tx_out(value, script_addr))
            .collect(),
    })
}

pub fn generate_input_bytes(
    tx_ins: Vec<(&str, u32, Option<&str>)>,
    tx_outs: Vec<(&str, u64)>,
) -> Vec<u8> {
    let mut bytes = generate_transaction_bytes(tx_ins, tx_outs);
    let mut sign_type = vec![1, 0, 0, 0];
    bytes.append(&mut sign_type);
    bytes
}

#[cfg(feature = "zcash")]
pub fn generate_zcash_transaction_bytes(
    tx_ins: Vec<(&str, u32, Option<&str>)>,
    tx_outs: Vec<(&str, u64)>,
) -> Vec<u8> {
    use zcash_primitives::consensus::{BlockHeight, BranchId};
    use zcash_primitives::transaction::{TransactionData, TxVersion};
    use zcash_transparent::bundle::{Authorized, OutPoint as ZcashOutPoint, TxIn as ZcashTxIn, TxOut as ZcashTxOut};

    // Create transparent inputs
    let zcash_inputs: Vec<ZcashTxIn<Authorized>> = tx_ins
        .into_iter()
        .map(|(tx_id, vout, script_addr)| {
            // Parse the txid string to bytes
            let txid_bytes = hex::decode(tx_id).expect("Invalid txid hex");
            let mut txid_array = [0u8; 32];
            txid_array.copy_from_slice(&txid_bytes);

            let prevout = ZcashOutPoint::new(txid_array, vout);

            // Create script_sig from address if provided
            let script_sig = if let Some(addr) = script_addr {
                let address = Address::from_str(addr)
                    .expect("Invalid btc address")
                    .assume_checked();
                zcash_transparent::address::Script(address.script_pubkey().to_bytes())
            } else {
                zcash_transparent::address::Script(vec![])
            };

            ZcashTxIn {
                prevout,
                script_sig,
                sequence: 4294967293, // Same as Bitcoin version
            }
        })
        .collect();

    // Create transparent outputs
    let zcash_outputs: Vec<ZcashTxOut> = tx_outs
        .into_iter()
        .map(|(script_addr, value)| {
            // Try to parse as Zcash address first, fallback to Bitcoin address
            let script_pubkey = if script_addr.starts_with('t') || script_addr.starts_with('z') {
                // Zcash address - decode it properly
                use zcash_address::ZcashAddress;
                match ZcashAddress::try_from_encoded(script_addr) {
                    Ok(zcash_addr) => {
                        // Use zcash_address crate to properly convert to transparent address
                        use zcash_protocol::consensus::NetworkType;
                        let (_network, addr_data) = zcash_addr.convert::<(NetworkType, zcash_transparent::address::TransparentAddress)>()
                            .expect("Failed to convert Zcash address to transparent address");

                        // Create script from the transparent address
                        let script_bytes = match addr_data {
                            zcash_transparent::address::TransparentAddress::PublicKeyHash(hash) => {
                                // P2PKH: OP_DUP OP_HASH160 <hash> OP_EQUALVERIFY OP_CHECKSIG
                                let mut script = vec![0x76, 0xa9, 0x14]; // OP_DUP, OP_HASH160, push 20 bytes
                                script.extend_from_slice(&hash);
                                script.push(0x88); // OP_EQUALVERIFY
                                script.push(0xac); // OP_CHECKSIG
                                script
                            },
                            zcash_transparent::address::TransparentAddress::ScriptHash(hash) => {
                                // P2SH: OP_HASH160 <hash> OP_EQUAL
                                let mut script = vec![0xa9, 0x14]; // OP_HASH160, push 20 bytes
                                script.extend_from_slice(&hash);
                                script.push(0x87); // OP_EQUAL
                                script
                            },
                        };
                        zcash_transparent::address::Script(script_bytes)
                    },
                    Err(_) => panic!("Invalid Zcash address: {}", script_addr),
                }
            } else {
                // Bitcoin address
                let address = Address::from_str(script_addr)
                    .expect("Invalid btc address")
                    .assume_checked();
                zcash_transparent::address::Script(address.script_pubkey().to_bytes())
            };

            ZcashTxOut {
                value: zcash_protocol::value::Zatoshis::const_from_u64(value),
                script_pubkey,
            }
        })
        .collect();

    // Create transparent bundle
    let transparent_bundle = zcash_transparent::bundle::Bundle {
        vin: zcash_inputs,
        vout: zcash_outputs,
        authorization: zcash_transparent::bundle::Authorized,
    };

    // Build Zcash v5 transaction
    let tx_data = TransactionData::from_parts(
        TxVersion::V5,               // V5
        BranchId::Nu6,               // Use Nu6 branch
        0,                           // lock_time
        BlockHeight::from_u32(2000), // expiry_height (must be > 0 for v5)
        Some(transparent_bundle),
        None, // sapling
        None, // orchard_v5
        None, // orchard (will be added later in withdrawal flow)
    );

    // Freeze the transaction to get the final Transaction object
    let tx = tx_data.freeze().expect("Failed to freeze transaction");

    // Serialize the transaction
    let mut buf = Vec::new();
    tx.write(&mut buf).expect("Failed to serialize Zcash transaction");

    buf
}

pub fn tool_err_msg(outcome: &Result<ExecutionFinalResult>) -> String {
    match outcome {
        Ok(res) => {
            let mut msg = "".to_string();
            for r in res.receipt_failures() {
                match r.clone().into_result() {
                    Ok(_) => {}
                    Err(err) => {
                        msg += &format!("{:?}", err);
                        msg += "\n";
                    }
                }
            }
            msg
        }
        Err(err) => err.to_string(),
    }
}

#[macro_export]
macro_rules! check {
    ($exec_func: expr) => {
        let outcome = $exec_func.await.unwrap();
        assert!(outcome.is_success() && outcome.receipt_failures().is_empty());
    };
    (print $exec_func: expr) => {
        let outcome = $exec_func.await;
        let err_msg = tool_err_msg(&outcome);
        println!("==>");
        if err_msg.is_empty() {
            let o = outcome.unwrap();
            println!("logs: {:#?}", o.logs());
        } else {
            println!("errors: {}", err_msg);
        }
        println!("<==");
    };
    (print $prefix: literal $exec_func: expr) => {
        let outcome = $exec_func.await;
        let err_msg = tool_err_msg(&outcome);
        println!("==>");
        if err_msg.is_empty() {
            let o = outcome.unwrap();
            println!("{} logs: {:#?}", $prefix, o.logs());
        } else {
            println!("{} errors: {}", $prefix, err_msg);
        }
        println!("<==");
    };
    (printr $exec_func: expr) => {
        let outcome = $exec_func.await;
        let err_msg = tool_err_msg(&outcome);
        println!("==>");
        if err_msg.is_empty() {
            let o = outcome.unwrap();
            println!("logs: {:#?}", o.logs());
            println!("");
            println!("return: {:#?}", o.json::<near_sdk::serde_json::Value>());
        } else {
            println!("errors: {}", err_msg);
        }
        println!("<==");
    };
    (printr $prefix: literal $exec_func: expr) => {
        let outcome = $exec_func.await;
        let err_msg = tool_err_msg(&outcome);
        println!("==>");
        if err_msg.is_empty() {
            let o = outcome.unwrap();
            println!("{} logs: {:#?}", $prefix, o.logs());
            println!("");
            println!(
                "{} return: {:#?}",
                $prefix,
                o.json::<near_sdk::serde_json::Value>()
            );
        } else {
            println!("{} errors: {}", $prefix, err_msg);
        }
        println!("<==");
    };
    (view $exec_func: expr) => {
        let query_result = $exec_func.await.unwrap();
        println!("{:?}", query_result);
    };
    (view $prefix: literal $exec_func: expr) => {
        let query_result = $exec_func.await.unwrap();
        println!("{} {:#?}", $prefix, query_result);
    };
    ($exec_func: expr, $err_info: expr) => {
        assert!(tool_err_msg(&$exec_func.await).contains($err_info));
    };
}
