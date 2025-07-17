#[cfg(not(feature = "zcash"))]
mod transaction_impl {
    use bitcoin::consensus::{Decodable, Encodable};
    use bitcoin::{absolute, Transaction as BtcTransaction, TxOut, Txid};

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Transaction {
        pub inner_tx: BtcTransaction,
    }

    impl Transaction {
        pub fn compute_txid(&self) -> Txid {
            self.inner_tx.compute_txid()
        }

        pub fn output(&self) -> Vec<TxOut> {
            self.inner_tx.output.clone()
        }

        pub fn lock_time(&self) -> absolute::LockTime {
            self.inner_tx.lock_time
        }

        pub fn encode(&self) -> Result<Vec<u8>, bitcoin::io::Error> {
            let mut buf = Vec::new();
            self.inner_tx.consensus_encode(&mut buf)?;
            Ok(buf)
        }

        pub fn decode(data: &[u8]) -> Result<Self, bitcoin::consensus::encode::Error> {
            let mut cursor = bitcoin::io::Cursor::new(data);
            let tx = BtcTransaction::consensus_decode(&mut cursor)?;
            Ok(Self { inner_tx: tx })
        }
    }
}

#[cfg(feature = "zcash")]
mod transaction_impl {
    use bitcoin::hashes::Hash;
    use bitcoin::{absolute, ScriptBuf, TxOut, Txid};
    use zcash_primitives::consensus::BranchId;
    use zcash_primitives::transaction::Transaction as ZCashTransaction;

    #[derive(Debug, PartialEq)]
    pub struct Transaction {
        pub inner_tx: ZCashTransaction,
    }

    impl Transaction {
        pub fn compute_txid(&self) -> Txid {
            Txid::from_byte_array(*self.inner_tx.txid().as_ref())
        }

        pub fn output(&self) -> Vec<TxOut> {
            let outputs = self.inner_tx.transparent_bundle().unwrap().vout.clone();
            outputs
                .into_iter()
                .map(|o| bitcoin::TxOut {
                    value: bitcoin::Amount::from_sat(o.value.into_u64()),
                    script_pubkey: ScriptBuf::from(bitcoin::Script::from_bytes(&o.script_pubkey.0)),
                })
                .collect()
        }

        pub fn lock_time(&self) -> absolute::LockTime {
            let lock_time = self.inner_tx.lock_time();
            absolute::LockTime::from_consensus(lock_time)
        }

        pub fn encode(&self) -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();
            self.inner_tx.write(&mut buf)?;

            Ok(buf)
        }

        pub fn decode(data: &[u8]) -> Result<Self, std::io::Error> {
            let mut cursor = std::io::Cursor::new(data);
            let tx = ZCashTransaction::read(&mut cursor, BranchId::Nu6)?;
            Ok(Self { inner_tx: tx })
        }
    }
}

#[cfg(not(feature = "zcash"))]
pub use transaction_impl::Transaction;

#[cfg(feature = "zcash")]
pub use transaction_impl::Transaction;
