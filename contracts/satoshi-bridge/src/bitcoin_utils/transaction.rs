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
