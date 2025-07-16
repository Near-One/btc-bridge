#[cfg(not(feature = "zcash"))]
mod transaction_impl {
    use bitcoin::consensus::{encode, Decodable, Encodable};
    use bitcoin::io::{Read, Write};
    use bitcoin::{absolute, io, OutPoint, Transaction as BtcTransaction, TxOut, Txid};

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
    }

    impl Encodable for Transaction {
        fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, io::Error> {
            Ok(self.inner_tx.consensus_encode(w)?)
        }
    }
    impl Decodable for Transaction {
        fn consensus_decode<R: Read + ?Sized>(r: &mut R) -> Result<Self, encode::Error> {
            Ok(Self {
                inner_tx: Decodable::consensus_decode(r)?,
            })
        }
    }
}

#[cfg(feature = "zcash")]
mod transaction_impl {
    use bitcoin::consensus::{encode, Decodable, Encodable};
    use bitcoin::hashes::{sha256, Hash};
    use bitcoin::io::{Read, Write};
    use bitcoin::{absolute, io, OutPoint, ScriptBuf, Transaction as BtcTransaction, TxOut, Txid};
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
    }

    impl Encodable for Transaction {
        fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, io::Error> {
            let mut buf = Vec::new();

            self.inner_tx.write(&mut buf).unwrap();

            bitcoin::io::Write::write_all(w, &buf)?;
            Ok(buf.len())
        }
    }
    impl Decodable for Transaction {
        fn consensus_decode<R: Read + ?Sized>(r: &mut R) -> Result<Self, encode::Error> {
            let mut buf = Vec::new();
            r.read_to_limit(&mut buf, 100000).unwrap();

            println!("Buffer: {:?}", buf);
            let mut cursor = std::io::Cursor::new(buf);

            let tx = ZCashTransaction::read(&mut cursor, BranchId::Nu6).unwrap();

            Ok(Self { inner_tx: tx })
        }
    }
}

#[cfg(not(feature = "zcash"))]
pub use transaction_impl::Transaction;

#[cfg(feature = "zcash")]
pub use transaction_impl::Transaction;
