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

        pub fn tx_bytes_with_sign(tx: bitcoin::Transaction) -> Result<Vec<u8>, bitcoin::io::Error> {
            Transaction { inner_tx: tx }.encode()
        }
    }
}

#[cfg(feature = "zcash")]
mod transaction_impl {
    use bitcoin::hashes::Hash;
    use bitcoin::{absolute, ScriptBuf, TxOut, Txid};
    use zcash_primitives::consensus::{BlockHeight, BranchId};
    use zcash_primitives::transaction::{
        Transaction as ZCashTransaction, TransactionData, TxVersion,
    };
    use zcash_protocol::value::Zatoshis;

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

        pub fn tx_bytes_with_sign(tx: bitcoin::Transaction) -> Result<Vec<u8>, std::io::Error> {
            let transparent_bundle = zcash_transparent::bundle::Bundle {
                vin: tx
                    .input
                    .iter()
                    .map(|input| zcash_transparent::bundle::TxIn {
                        prevout: zcash_transparent::bundle::OutPoint::new(
                            input.previous_output.txid.to_byte_array(),
                            input.previous_output.vout,
                        ),
                        script_sig: zcash_primitives::legacy::Script(input.script_sig.to_bytes()),
                        sequence: input.sequence.0,
                    })
                    .collect(),
                vout: tx
                    .output
                    .iter()
                    .map(|output| zcash_transparent::bundle::TxOut {
                        value: Zatoshis::from_u64(output.value.to_sat()).unwrap(),
                        script_pubkey: zcash_primitives::legacy::Script(
                            output.script_pubkey.to_bytes(),
                        ),
                    })
                    .collect(),
                authorization: zcash_transparent::bundle::Authorized,
            };

            let lock_time = 0;
            let expiry_height = BlockHeight::from_u32(0);
            let inner_tx = TransactionData::from_parts(
                TxVersion::V5,
                BranchId::Nu6,
                lock_time,
                expiry_height,
                Some(transparent_bundle),
                None,
                None,
                None,
            )
            .freeze()
            .unwrap();

            Transaction { inner_tx }.encode()
        }
    }
}

#[cfg(not(feature = "zcash"))]
pub use transaction_impl::Transaction;

#[cfg(feature = "zcash")]
pub use transaction_impl::Transaction;
