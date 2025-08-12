use bitcoin::hashes::Hash;
use bitcoin::{absolute, ScriptBuf, TxOut, Txid};
use zcash_primitives::consensus::{BlockHeight, BranchId};
use zcash_primitives::transaction::{
    Transaction as ZCashTransaction, TransactionData, TxVersion, Unauthorized,
};
use zcash_transparent::builder::TransparentBuilder;
use zcash_transparent::bundle::Authorized;

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

    pub fn get_transparent_builder(
        vin: &Vec<zcash_transparent::bundle::TxIn<Authorized>>,
        vout: &Vec<zcash_transparent::bundle::TxOut>,
        input: &Vec<zcash_transparent::bundle::TxOut>,
        public_key: &bitcoin::PublicKey,
    ) -> TransparentBuilder {
        let mut builder = zcash_transparent::builder::TransparentBuilder::empty();

        for index in 0..vin.len() {
            builder
                .add_input(
                    public_key.inner,
                    vin[index].prevout.clone(),
                    input[index].clone(),
                )
                .unwrap();
        }

        for output in vout {
            let key = output.script_pubkey.0[3..23].try_into().unwrap();
            let to = zcash_transparent::address::TransparentAddress::PublicKeyHash(key);
            builder.add_output(&to, output.value).unwrap();
        }

        builder
    }

    pub fn to_zcash_tx(
        vin: &Vec<zcash_transparent::bundle::TxIn<Authorized>>,
        vout: &Vec<zcash_transparent::bundle::TxOut>,
        input: &Vec<zcash_transparent::bundle::TxOut>,
        expiry_height: u32,
        public_key: &bitcoin::PublicKey,
    ) -> TransactionData<Unauthorized> {
        let transparent_bundle = Self::get_transparent_builder(vin, vout, input, public_key)
            .build()
            .unwrap();

        let lock_time = 0;
        let expiry_height = BlockHeight::from_u32(expiry_height);
        let inner_tx = TransactionData::from_parts(
            TxVersion::V5,
            BranchId::Nu6,
            lock_time,
            expiry_height,
            Some(transparent_bundle),
            None,
            None,
            None,
        );

        inner_tx
    }
}
