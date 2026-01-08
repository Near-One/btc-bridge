use crate::network;
use bitcoin::hashes::Hash;
use bitcoin::{absolute, ScriptBuf, TxOut, Txid};
use zcash_primitives::consensus::{BlockHeight, BranchId};
use zcash_primitives::transaction::{Transaction as ZCashTransaction, TransactionData, TxVersion};
use zcash_transparent::builder::TransparentBuilder;
use zcash_transparent::bundle::Authorized;

pub struct TransparentUnauthorized;
impl zcash_primitives::transaction::Authorization for TransparentUnauthorized {
    type TransparentAuth = zcash_transparent::builder::Unauthorized;
    type SaplingAuth = sapling_crypto::bundle::Authorized;
    type OrchardAuth = orchard::bundle::Authorized;
}

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

    pub fn decode(data: &[u8], chain: &network::Chain) -> Result<Self, std::io::Error> {
        let mut cursor = std::io::Cursor::new(data);
        let branch_id = match chain {
            network::Chain::ZcashTestnet => BranchId::Nu6_1,
            _ => BranchId::Nu6,
        };
        let tx = ZCashTransaction::read(&mut cursor, branch_id)?;
        Ok(Self { inner_tx: tx })
    }

    pub fn get_transparent_builder(
        vin: &[zcash_transparent::bundle::TxIn<Authorized>],
        vout: &[zcash_transparent::bundle::TxOut],
        input: &[zcash_transparent::bundle::TxOut],
        public_keys: &[bitcoin::PublicKey],
    ) -> TransparentBuilder {
        let mut builder = zcash_transparent::builder::TransparentBuilder::empty();

        for index in 0..vin.len() {
            builder
                .add_input(
                    public_keys[index].inner,
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
        vin: &[zcash_transparent::bundle::TxIn<Authorized>],
        vout: &[zcash_transparent::bundle::TxOut],
        input: &[zcash_transparent::bundle::TxOut],
        expiry_height: u32,
        public_keys: &[bitcoin::PublicKey],
        branch_id: BranchId,
    ) -> TransactionData<TransparentUnauthorized> {
        let transparent_bundle = Self::get_transparent_builder(vin, vout, input, public_keys)
            .build()
            .unwrap();

        let lock_time = 0;
        let expiry_height = BlockHeight::from_u32(expiry_height);

        TransactionData::from_parts(
            TxVersion::V5,
            branch_id,
            lock_time,
            expiry_height,
            Some(transparent_bundle),
            None,
            None,
            None,
        )
    }
}
