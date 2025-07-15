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
    use zebra_chain::serialization::SerializationError::Amount;
    use zebra_chain::serialization::{ZcashDeserialize, ZcashSerialize};
    use zebra_chain::transaction::Transaction as ZCashTransaction;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Transaction {
        pub inner_tx: ZCashTransaction,
    }

    impl Transaction {
        pub fn compute_txid(&self) -> Txid {
            Txid::from_byte_array(self.inner_tx.hash().0)
        }

        pub fn output(&self) -> Vec<TxOut> {
            let outputs = self.inner_tx.outputs().clone();
            outputs
                .into_iter()
                .map(|o| {
                    let mut bytes = Vec::new();
                    o.lock_script.zcash_serialize(&mut bytes).unwrap();

                    bitcoin::TxOut {
                        value: bitcoin::Amount::from_sat(o.value.zatoshis() as u64),
                        script_pubkey: ScriptBuf::from(bitcoin::Script::from_bytes(&bytes)),
                    }
                })
                .collect()
        }

        pub fn lock_time(&self) -> absolute::LockTime {
            let lock_time = self.inner_tx.lock_time().unwrap();
            let mut data = Vec::new();
            &lock_time
                .zcash_serialize(&mut data)
                .expect("Serialization failed");

            absolute::LockTime::from_hex(&hex::encode(data)).unwrap()
        }
    }

    impl Encodable for Transaction {
        fn consensus_encode<W: Write + ?Sized>(&self, w: &mut W) -> Result<usize, io::Error> {
            let mut buf = Vec::new();

            self.inner_tx.zcash_serialize(&mut buf).unwrap();

            bitcoin::io::Write::write_all(w, &buf)?;
            Ok(buf.len())
        }
    }
    impl Decodable for Transaction {
        fn consensus_decode<R: Read + ?Sized>(r: &mut R) -> Result<Self, encode::Error> {
            let mut buf = Vec::new();
            bitcoin::io::Read::read(r, &mut buf)?;

            println!("Buffer: {:?}", buf);
            let mut cursor = std::io::Cursor::new(buf);

            let tx = ZCashTransaction::zcash_deserialize(&mut cursor).unwrap();

            Ok(Self { inner_tx: tx })
        }
    }
}

#[cfg(not(feature = "zcash"))]
pub use transaction_impl::Transaction;

#[cfg(feature = "zcash")]
pub use transaction_impl::Transaction;
