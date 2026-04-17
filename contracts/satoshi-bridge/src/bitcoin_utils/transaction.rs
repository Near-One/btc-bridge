use crate::network::{self, Address};
use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::{absolute, ScriptBuf, Transaction as BtcTransaction, TxOut, Txid};

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

    pub fn decode(
        data: &[u8],
        _chain: &network::Chain,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let mut cursor = bitcoin::io::Cursor::new(data);
        let tx = BtcTransaction::consensus_decode(&mut cursor)?;
        Ok(Self { inner_tx: tx })
    }

    pub fn tx_bytes_with_sign(tx: bitcoin::Transaction) -> Result<Vec<u8>, bitcoin::io::Error> {
        Transaction { inner_tx: tx }.encode()
    }

    /// Check if any transaction input was sent from the given address.
    ///
    /// Parses the address into a script_pubkey, then for each input extracts the pubkey
    /// and computes the matching script_pubkey to compare directly (no string conversion).
    ///
    /// Only checks inputs whose type matches the address type:
    /// - P2WPKH address: checks only witness inputs (witness = [signature, pubkey]).
    /// - P2PKH address: checks only legacy inputs (script_sig ends with 0x21 + 33-byte pubkey).
    ///
    /// Returns false for unsupported address/input types (P2SH, P2TR, multisig, etc.).
    pub fn has_input_from_address(&self, address: &str, chain: &network::Chain) -> bool {
        let parsed = Address::parse(address, chain.clone()).expect("Invalid refund address");
        let target_script = parsed
            .script_pubkey()
            .expect("Invalid refund script_pubkey");

        match parsed {
            Address::Segwit { .. } => self.inner_tx.input.iter().any(|input| {
                if input.witness.len() != 2 {
                    return false;
                }
                input
                    .witness
                    .last()
                    .and_then(|bytes| bitcoin::CompressedPublicKey::from_slice(bytes).ok())
                    .is_some_and(|pubkey| {
                        ScriptBuf::new_p2wpkh(&pubkey.wpubkey_hash()) == target_script
                    })
            }),
            Address::P2pkh { .. } => self.inner_tx.input.iter().any(|input| {
                let sig_bytes = input.script_sig.as_bytes();
                if sig_bytes.len() < 34 || sig_bytes[sig_bytes.len() - 34] != 0x21 {
                    return false;
                }
                bitcoin::PublicKey::from_slice(&sig_bytes[sig_bytes.len() - 33..])
                    .ok()
                    .is_some_and(|pubkey| {
                        ScriptBuf::new_p2pkh(&pubkey.pubkey_hash()) == target_script
                    })
            }),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{
        absolute::LockTime, transaction::Version, CompressedPublicKey, Sequence, TxIn, Witness,
    };
    use network::Chain;

    /// A known compressed public key for testing.
    /// Corresponding P2WPKH (testnet): tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx
    /// Corresponding P2PKH  (testnet): mrCDrCybB6J1vRfbwM5hemdJz73FwDBC8r
    const TEST_PUBKEY_HEX: &str =
        "0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798";

    fn test_pubkey_bytes() -> Vec<u8> {
        hex::decode(TEST_PUBKEY_HEX).unwrap()
    }

    /// Fake 71-byte DER signature (just needs to be plausible length).
    fn fake_signature() -> Vec<u8> {
        vec![0x30; 71]
    }

    /// Build a transaction with a single P2WPKH input (witness = [sig, pubkey]).
    fn tx_with_p2wpkh_input() -> Transaction {
        let witness = Witness::from_slice(&[&fake_signature(), &test_pubkey_bytes()]);
        Transaction {
            inner_tx: BtcTransaction {
                version: Version::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: Default::default(),
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness,
                }],
                output: vec![],
            },
        }
    }

    /// Build a transaction with a single P2PKH input (script_sig = <push sig> <sig> 0x21 <pubkey>).
    fn tx_with_p2pkh_input() -> Transaction {
        let sig = fake_signature();
        // script_sig: <push sig_len> <sig> <0x21> <33-byte pubkey>
        let mut script_bytes = vec![sig.len() as u8];
        script_bytes.extend_from_slice(&sig);
        script_bytes.push(0x21);
        script_bytes.extend_from_slice(&test_pubkey_bytes());
        Transaction {
            inner_tx: BtcTransaction {
                version: Version::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: Default::default(),
                    script_sig: ScriptBuf::from_bytes(script_bytes),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                }],
                output: vec![],
            },
        }
    }

    fn p2wpkh_address() -> String {
        let pubkey = CompressedPublicKey::from_slice(&test_pubkey_bytes()).unwrap();
        let wp = bitcoin::WitnessProgram::p2wpkh(&pubkey);
        // Encode as testnet bech32
        let addr = bitcoin::Address::from_witness_program(wp, bitcoin::Network::Testnet);
        addr.to_string()
    }

    fn p2pkh_address() -> String {
        let pubkey = bitcoin::PublicKey::from_slice(&test_pubkey_bytes()).unwrap();
        let addr = bitcoin::Address::p2pkh(pubkey, bitcoin::Network::Testnet);
        addr.to_string()
    }

    #[test]
    fn test_p2wpkh_input_matches_correct_address() {
        let tx = tx_with_p2wpkh_input();
        let addr = p2wpkh_address();
        assert!(
            tx.has_input_from_address(&addr, &Chain::BitcoinTestnet),
            "P2WPKH input should match its own address: {addr}"
        );
    }

    #[test]
    fn test_p2pkh_input_matches_correct_address() {
        let tx = tx_with_p2pkh_input();
        let addr = p2pkh_address();
        assert!(
            tx.has_input_from_address(&addr, &Chain::BitcoinTestnet),
            "P2PKH input should match its own address: {addr}"
        );
    }

    /// Generate a different address from a different pubkey.
    fn other_pubkey_bytes() -> Vec<u8> {
        // secp256k1 generator point G * 2 (a valid compressed pubkey different from TEST_PUBKEY)
        hex::decode("02C6047F9441ED7D6D3045406E95C07CD85C778E4B8CEF3CA7ABAC09B95C709EE5").unwrap()
    }

    fn other_p2wpkh_address() -> String {
        let pubkey = CompressedPublicKey::from_slice(&other_pubkey_bytes()).unwrap();
        let wp = bitcoin::WitnessProgram::p2wpkh(&pubkey);
        bitcoin::Address::from_witness_program(wp, bitcoin::Network::Testnet).to_string()
    }

    fn other_p2pkh_address() -> String {
        let pubkey = bitcoin::PublicKey::from_slice(&other_pubkey_bytes()).unwrap();
        bitcoin::Address::p2pkh(pubkey, bitcoin::Network::Testnet).to_string()
    }

    #[test]
    fn test_p2wpkh_input_does_not_match_wrong_address() {
        let tx = tx_with_p2wpkh_input();
        let wrong_addr = other_p2wpkh_address();
        assert!(
            !tx.has_input_from_address(&wrong_addr, &Chain::BitcoinTestnet),
            "P2WPKH input should not match a different address"
        );
    }

    #[test]
    fn test_p2pkh_input_does_not_match_wrong_address() {
        let tx = tx_with_p2pkh_input();
        let wrong_addr = other_p2pkh_address();
        assert!(
            !tx.has_input_from_address(&wrong_addr, &Chain::BitcoinTestnet),
            "P2PKH input should not match a different address"
        );
    }

    #[test]
    fn test_p2wpkh_input_does_not_match_p2pkh_address() {
        let tx = tx_with_p2wpkh_input();
        // Same pubkey but P2PKH address — type mismatch, should not match
        let p2pkh_addr = p2pkh_address();
        assert!(
            !tx.has_input_from_address(&p2pkh_addr, &Chain::BitcoinTestnet),
            "P2WPKH input should not match a P2PKH address even for the same pubkey"
        );
    }

    #[test]
    fn test_p2pkh_input_does_not_match_p2wpkh_address() {
        let tx = tx_with_p2pkh_input();
        // Same pubkey but P2WPKH address — type mismatch, should not match
        let p2wpkh_addr = p2wpkh_address();
        assert!(
            !tx.has_input_from_address(&p2wpkh_addr, &Chain::BitcoinTestnet),
            "P2PKH input should not match a P2WPKH address even for the same pubkey"
        );
    }
}
