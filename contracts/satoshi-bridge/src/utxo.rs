use crate::{
    generate_utxo_storage_key, near, psbt_wrapper::PsbtWrapper, u64_dec_format, Contract, OutPoint,
};
use near_sdk::env;
use near_sdk::json_types::Base64VecU8;

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct UTXO {
    pub path: String,
    pub tx_bytes: Base64VecU8,
    pub vout: usize,
    #[serde(with = "u64_dec_format")]
    pub balance: u64,
}

/// View-only mirror of [`UTXO`] that serializes `tx_bytes` as a JSON byte array
/// for backward compatibility with off-chain clients (relayer).
#[near(serializers = [json])]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct UTXOView {
    pub path: String,
    pub tx_bytes: Vec<u8>,
    pub vout: usize,
    #[serde(with = "u64_dec_format")]
    pub balance: u64,
}

impl From<&UTXO> for UTXOView {
    fn from(u: &UTXO) -> Self {
        UTXOView {
            path: u.path.clone(),
            tx_bytes: u.tx_bytes.0.clone(),
            vout: u.vout,
            balance: u.balance,
        }
    }
}

/// View-only mirror of [`VUTXO`] preserving the legacy JSON shape (`{"Current": {...}}`).
#[near(serializers = [json])]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub enum VUTXOView {
    Current(UTXOView),
}

impl From<&VUTXO> for VUTXOView {
    fn from(v: &VUTXO) -> Self {
        match v {
            VUTXO::Current(c) => VUTXOView::Current(c.into()),
        }
    }
}

impl From<&VUTXO> for UTXOView {
    fn from(v: &VUTXO) -> Self {
        match v {
            VUTXO::Current(c) => c.into(),
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub enum VUTXO {
    Current(UTXO),
}

impl VUTXO {
    pub fn get_path(&self) -> String {
        match self {
            VUTXO::Current(c) => c.path.clone(),
        }
    }

    pub fn get_amount(&self) -> u64 {
        match self {
            VUTXO::Current(c) => c.balance,
        }
    }
}

impl From<VUTXO> for UTXO {
    fn from(v: VUTXO) -> Self {
        match v {
            VUTXO::Current(c) => c,
        }
    }
}

impl From<&VUTXO> for UTXO {
    fn from(v: &VUTXO) -> Self {
        match v {
            VUTXO::Current(c) => c.clone(),
        }
    }
}

impl From<UTXO> for VUTXO {
    fn from(c: UTXO) -> Self {
        VUTXO::Current(c)
    }
}

impl Contract {
    pub fn remove_vutxo_by_psbt(&mut self, psbt: &PsbtWrapper) -> (Vec<String>, Vec<VUTXO>) {
        let mut utxo_storage_keys = vec![];
        let vutxos = psbt
            .get_utxo_storage_keys()
            .into_iter()
            .map(|utxo_storage_key| {
                utxo_storage_keys.push(utxo_storage_key.clone());
                self.data_mut()
                    .utxos
                    .remove(&utxo_storage_key)
                    .unwrap_or_else(|| {
                        env::panic_str(&format!("UTXO {} not exist", utxo_storage_key))
                    })
            })
            .collect::<Vec<_>>();
        (utxo_storage_keys, vutxos)
    }
}

impl Contract {
    pub fn internal_set_utxo(&mut self, utxo_storage_key: &str, utxo: UTXO) {
        self.data_mut()
            .utxos
            .insert(utxo_storage_key.to_owned(), utxo.into());
    }

    pub fn internal_set_unavailable_utxo(&mut self, utxo_storage_key: &str, utxo: UTXO) {
        self.data_mut()
            .unavailable_utxos
            .insert(utxo_storage_key.to_owned(), utxo.into());
    }
}

#[near(serializers = [json])]
pub struct PendingUTXOInfo {
    pub tx_id: String,
    pub utxo_storage_key: String,
    pub utxo: UTXO,
}

pub fn out_point_to_utxo_storage_key(out_point: &OutPoint) -> String {
    generate_utxo_storage_key(out_point.txid.to_string(), out_point.vout)
}
