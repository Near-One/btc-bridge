use bitcoin::bech32::Hrp;
use bitcoin::hashes::Hash;
use bitcoin::{base58, bech32, PubkeyHash, ScriptHash, WitnessProgram, WitnessVersion};
use near_sdk::near;
use std::fmt;
use zcash_address::unified::{Container, Receiver};
use zcash_address::{ConversionError, ToAddress, ZcashAddress};
#[cfg(feature = "zcash")]
use zcash_protocol::consensus::BranchId;

/// Size of Orchard raw address bytes (diversifier + pk_d).
pub const ORCHARD_RAW_ADDRESS_SIZE: usize = 43;

/// Type alias for Orchard raw address bytes to avoid magic numbers.
pub type OrchardRawAddress = [u8; ORCHARD_RAW_ADDRESS_SIZE];

#[near(serializers = [borsh, json])]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Chain {
    BitcoinMainnet,
    BitcoinTestnet,
    LitecoinMainnet,
    LitecoinTestnet,
    ZcashMainnet,
    ZcashTestnet,
    DogecoinMainnet,
    DogecoinTestnet,
}
#[cfg(feature = "zcash")]
pub struct BranchIdUpdateBlockHeight {
    pub nu6_1_update: u32,
    pub nu6_2_update: u32,
}

#[cfg(feature = "zcash")]
impl BranchIdUpdateBlockHeight {
    pub fn new(chain: &Chain) -> Self {
        match chain {
            Chain::ZcashMainnet => Self {
                nu6_1_update: 3146400,
                nu6_2_update: 3364600,
            },
            Chain::ZcashTestnet => Self {
                nu6_1_update: 3536500,
                nu6_2_update: 4052000,
            },
            _ => unreachable!(),
        }
    }
}
impl Chain {
    #[cfg(feature = "zcash")]
    pub fn get_branch_id(&self, block_height: u32) -> BranchId {
        let block_height_update = BranchIdUpdateBlockHeight::new(self);
        if block_height_update.nu6_2_update != 0 && block_height >= block_height_update.nu6_2_update
        {
            return BranchId::Nu6_2;
        }
        if block_height_update.nu6_1_update != 0 && block_height >= block_height_update.nu6_1_update
        {
            return BranchId::Nu6_1;
        }

        BranchId::Nu6
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Address {
    P2pkh {
        hash: PubkeyHash,
        chain: Chain,
    },
    P2sh {
        hash: ScriptHash,
        chain: Chain,
    },
    Segwit {
        program: WitnessProgram,
        chain: Chain,
    },
    Unified {
        address: zcash_address::unified::Address,
        chain: Chain,
    },
}

impl zcash_address::TryFromAddress for Address {
    type Error = String;
    fn try_from_transparent_p2pkh(
        net: zcash_protocol::consensus::NetworkType,
        data: [u8; 20],
    ) -> Result<Self, ConversionError<Self::Error>> {
        let chain = match net {
            zcash_protocol::consensus::NetworkType::Main => Chain::ZcashMainnet,
            zcash_protocol::consensus::NetworkType::Test => Chain::ZcashTestnet,
            zcash_protocol::consensus::NetworkType::Regtest => {
                return Err("Regtest network not supported".to_string().into());
            }
        };

        Ok(Self::P2pkh {
            hash: PubkeyHash::from_slice(&data[..]).map_err(|e| {
                ConversionError::<Self::Error>::from(format!("Invalid pubkey hash: {e}"))
            })?,
            chain,
        })
    }

    fn try_from_unified(
        net: zcash_protocol::consensus::NetworkType,
        data: zcash_address::unified::Address,
    ) -> Result<Self, ConversionError<Self::Error>> {
        let chain = match net {
            zcash_protocol::consensus::NetworkType::Main => Chain::ZcashMainnet,
            zcash_protocol::consensus::NetworkType::Test => Chain::ZcashTestnet,
            zcash_protocol::consensus::NetworkType::Regtest => {
                return Err("Regtest network not supported".to_string().into());
            }
        };

        Ok(Self::Unified {
            address: data,
            chain,
        })
    }
}

impl Address {
    /// Parse an address string + chain into `AddressInner`
    pub fn parse(address: &str, chain: Chain) -> Result<Self, String> {
        if chain == Chain::ZcashMainnet || chain == Chain::ZcashTestnet {
            let addr = ZcashAddress::try_from_encoded(address)
                .map_err(|e| format!("Error on parsing ZCash Address: {e}"))?;

            let network = match chain {
                Chain::ZcashMainnet => zcash_protocol::consensus::NetworkType::Main,
                Chain::ZcashTestnet => zcash_protocol::consensus::NetworkType::Test,
                _ => unreachable!(),
            };

            return addr
                .convert_if_network::<Self>(network)
                .map_err(|e| e.to_string());
        }

        if let Some(hrp) = get_segwit_hrp(&chain) {
            if let Ok((decoded_hrp, witness_version, data)) = bech32::segwit::decode(address) {
                let expected_hrp =
                    Hrp::parse(hrp).map_err(|e| format!("Invalid expected HRP '{hrp}': {e}"))?;
                if expected_hrp != decoded_hrp {
                    return Err(format!(
                        "Bech32 HRP mismatch: expected '{hrp}', got '{decoded_hrp}'"
                    ));
                }

                let version =
                    WitnessVersion::try_from(witness_version).map_err(|err| format!("{err:?}"))?;
                let program = WitnessProgram::new(version, &data).map_err(|err| {
                    format!("bech32 guarantees valid program length for witness: {err:?}")
                })?;

                return Ok(Address::Segwit { program, chain });
            }
        }

        let data = bitcoin::base58::decode_check(address)
            .map_err(|e| format!("Base58 decode error: {e}"))?;

        let prefix = get_pubkey_address_prefix(&chain);
        if data.starts_with(&prefix) {
            let hash = PubkeyHash::from_slice(&data[prefix.len()..])
                .map_err(|e| format!("Invalid pubkey hash: {e}"))?;
            return Ok(Address::P2pkh { hash, chain });
        }

        let prefix = get_script_address_prefix(&chain);
        if data.starts_with(&prefix) {
            let hash = ScriptHash::from_slice(&data[prefix.len()..])
                .map_err(|e| format!("Invalid script hash: {e}"))?;
            return Ok(Address::P2sh { hash, chain });
        }

        Err("Unknown address format or unsupported chain".to_string())
    }

    /// Return the scriptPubKey corresponding to this address
    pub fn script_pubkey(&self) -> Result<bitcoin::ScriptBuf, String> {
        match self {
            Address::P2pkh { hash, .. } => Ok(bitcoin::ScriptBuf::new_p2pkh(hash)),
            Address::P2sh { hash, .. } => Ok(bitcoin::ScriptBuf::new_p2sh(hash)),
            Address::Segwit { program, .. } => Ok(bitcoin::ScriptBuf::new_witness_program(program)),
            Address::Unified { address, .. } => {
                let receiver_list = address.items_as_parsed();
                for receiver in receiver_list {
                    match receiver {
                        Receiver::P2pkh(data) => {
                            return Ok(bitcoin::ScriptBuf::new_p2pkh(
                                &PubkeyHash::from_slice(&data[..]).map_err(|err| {
                                    format!("Error on parsing Pubkey Hash: {err:?}").to_string()
                                })?,
                            ))
                        }
                        Receiver::P2sh(data) => {
                            return Ok(bitcoin::ScriptBuf::new_p2sh(
                                &ScriptHash::from_slice(&data[..]).map_err(|err| {
                                    format!("Error on parsing Script Hash: {err:?}").to_string()
                                })?,
                            ))
                        }
                        _ => {}
                    }
                }

                Err("No receiver found in address".to_string())
            }
        }
    }

    /// Extract the Orchard receiver raw bytes from a Unified Address string for the given chain.
    pub fn extract_orchard_receiver(&self) -> Result<OrchardRawAddress, String> {
        match self {
            Address::Unified { address, .. } => {
                let receiver_list = address.items_as_parsed();
                for receiver in receiver_list {
                    match receiver {
                        Receiver::Orchard(bytes) => return Ok(*bytes),
                        _ => continue,
                    }
                }

                Err("Unified address missing Orchard receiver".to_string())
            }
            _ => Err("No Orchard address found".to_string()),
        }
    }

    pub fn from_pubkey(chain: Chain, pubkey: bitcoin::PublicKey) -> Result<Self, String> {
        let pubkey_hash = pubkey.pubkey_hash();

        if let Some(_hrp) = get_segwit_hrp(&chain) {
            // Chain supports Bech32 SegWit
            let wp = WitnessProgram::p2wpkh(
                &pubkey
                    .try_into()
                    .map_err(|e| format!("Error on converting pubkey to bytes: {e}"))?,
            );
            let wp = WitnessProgram::new(WitnessVersion::V0, wp.program().as_bytes())
                .map_err(|e| format!("bech32 guarantees valid program length for witness: {e}"))?;
            Ok(Address::Segwit { program: wp, chain })
        } else {
            // Legacy P2PKH
            Ok(Address::P2pkh {
                hash: pubkey_hash,
                chain,
            })
        }
    }

    pub fn from_script(script: &bitcoin::Script, chain: Chain) -> Option<Self> {
        // Try P2PKH
        if script.is_p2pkh() {
            let hash = bitcoin::PubkeyHash::from_slice(&script.as_bytes()[3..23]).ok()?;
            return Some(Address::P2pkh { hash, chain });
        }

        // Try P2SH
        if script.is_p2sh() {
            let hash = bitcoin::ScriptHash::from_slice(&script.as_bytes()[2..22]).ok()?;
            return Some(Address::P2sh { hash, chain });
        }

        if script.is_witness_program() {
            let opcode = script.first_opcode()?;

            let version = WitnessVersion::try_from(opcode).ok()?;
            let program = WitnessProgram::new(version, &script.as_bytes()[2..]).ok()?;
            return Some(Address::Segwit { program, chain });
        }

        None
    }
}

/// Formats bech32 as upper case if alternate formatting is chosen (`{:#}`).
impl fmt::Display for Address {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use Address::{P2pkh, P2sh, Segwit, Unified};
        match self {
            P2pkh { hash, chain } => {
                let prefix = get_pubkey_address_prefix(chain);
                let mut prefixed = Vec::with_capacity(20 + prefix.len());
                prefixed.extend(prefix);
                prefixed.extend(&hash[..]);
                base58::encode_check_to_fmt(fmt, &prefixed[..])
            }
            P2sh { hash, chain } => {
                let prefix = get_script_address_prefix(chain);
                let mut prefixed = Vec::with_capacity(20 + prefix.len());
                prefixed.extend(prefix);
                prefixed.extend(&hash[..]);
                base58::encode_check_to_fmt(fmt, &prefixed[..])
            }
            Segwit { program, chain } => {
                let hrp =
                    Hrp::parse(get_segwit_hrp(chain).ok_or(fmt::Error)?).map_err(|_| fmt::Error)?;
                let version = program.version().to_fe();
                let program = program.program().as_ref();

                if fmt.alternate() {
                    bech32::segwit::encode_upper_to_fmt_unchecked(fmt, hrp, version, program)
                } else {
                    bech32::segwit::encode_lower_to_fmt_unchecked(fmt, hrp, version, program)
                }
            }
            Unified { address, chain } => {
                let network = match chain {
                    Chain::ZcashMainnet => zcash_protocol::consensus::NetworkType::Main,
                    Chain::ZcashTestnet => zcash_protocol::consensus::NetworkType::Test,
                    _ => unreachable!(),
                };

                let str_address = ZcashAddress::from_unified(network, address.clone()).encode();
                write!(fmt, "{str_address}")
            }
        }
    }
}

pub fn get_segwit_hrp(chain: &Chain) -> Option<&'static str> {
    match chain {
        // Bitcoin (Bech32 - BIP173)
        Chain::BitcoinMainnet => Some("bc"),
        Chain::BitcoinTestnet => Some("tb"),

        // Litecoin (Bech32)
        Chain::LitecoinMainnet => Some("ltc"),
        Chain::LitecoinTestnet => Some("tltc"),

        // Zcash (Bech32m) support unified addresses with hrp but not segwit
        Chain::ZcashMainnet | Chain::ZcashTestnet => None,

        // Dogecoin (no native Bech32 support yet)
        Chain::DogecoinMainnet | Chain::DogecoinTestnet => None,
    }
}

/// Returns the P2PKH (pubkey) address prefix as a Vec<u8>
fn get_pubkey_address_prefix(chain: &Chain) -> Vec<u8> {
    match chain {
        // Bitcoin
        Chain::BitcoinMainnet => vec![0x00], // "1"
        Chain::BitcoinTestnet => vec![0x6F], // "m" or "n"

        // Litecoin
        Chain::LitecoinMainnet => vec![0x30], // "L"
        Chain::LitecoinTestnet => vec![0x6F],

        // Zcash
        Chain::ZcashMainnet => vec![0x1C, 0xB8], // "t1"
        Chain::ZcashTestnet => vec![0x1D, 0x25], // "tm"

        // Dogecoin
        Chain::DogecoinMainnet => vec![0x1E], // "D"
        Chain::DogecoinTestnet => vec![0x71], // "n"
    }
}

/// Returns the P2SH (script) address prefix as a Vec<u8>
fn get_script_address_prefix(chain: &Chain) -> Vec<u8> {
    match chain {
        // Bitcoin
        Chain::BitcoinMainnet => vec![0x05], // "3"
        Chain::BitcoinTestnet => vec![0xC4], // "2"

        // Litecoin
        Chain::LitecoinMainnet => vec![0x32], // "M" (was "3")
        Chain::LitecoinTestnet => vec![0x3A],

        // Zcash
        Chain::ZcashMainnet => vec![0x1C, 0xBD], // "t3"
        Chain::ZcashTestnet => vec![0x1C, 0xBA], // "t2"

        // Dogecoin
        Chain::DogecoinMainnet => vec![0x16], // "9"
        Chain::DogecoinTestnet => vec![0xC4], // same as Bitcoin testnet
    }
}

#[cfg(test)]
mod tests {
    use crate::network::{Address, Chain};
    use bitcoin::PublicKey as BtcPublicKey;
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    use near_sdk::PublicKey;
    use std::str::FromStr;
    pub fn generate_public_key(path: &str) -> Vec<u8> {
        let mpc_pk = crypto_shared::near_public_key_to_affine_point(
            PublicKey::from_str("secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3").unwrap(),
        );
        let epsilon = crypto_shared::derive_epsilon(
            &"zcash_connector-20250714-143829.testnet".parse().unwrap(),
            path,
        );
        let user_pk = crypto_shared::derive_key(mpc_pk, epsilon);
        let user_pk_encoded_point = user_pk.to_encoded_point(false);
        user_pk_encoded_point.as_bytes().to_vec()
    }

    pub fn generate_btc_public_key(path: &str) -> BtcPublicKey {
        let public_key_bytes = generate_public_key(path);
        let uncompressed_btc_public_key =
            BtcPublicKey::from_slice(&public_key_bytes).expect("Invalid public key bytes");
        uncompressed_btc_public_key
            .inner
            .to_string()
            .parse()
            .unwrap()
    }

    #[test]
    fn test_parse_address() {
        for (address, chain) in [
            (
                "bc1pwyzhgwy30q2juhau2f2c4qscasddle5ymw9m7scq5kc62t8kyzkqyz059k",
                Chain::BitcoinMainnet,
            ),
            (
                "tb1pt34385rvqtyuz6muh9hr5ed4fy0cx89zz0faxm6dhku0vqp2pxxs0ymh7y",
                Chain::BitcoinTestnet,
            ),
            ("LWrHnw5xztWiPafMhKYTQued8iuhaET7Yd", Chain::LitecoinMainnet),
            (
                "tltc1q0c8899qaxq4e5m9zucq9vkvrn4npfwa8pww9d8",
                Chain::LitecoinTestnet,
            ),
            ("t1ggQ7ZgHRoR34Z2xCcF155VcDe5zDZpZF1", Chain::ZcashMainnet),
            ("tmJpMbYtRf9Hgi8HUJ4FGkoM3FUSHsu28wM", Chain::ZcashTestnet),
            ("DKNmffVbxrBcNvQ9uJEDLe8f6prxSmH2Vm", Chain::DogecoinMainnet),
            ("njyMWWyh1L7tSX6QkWRgetMVCVyVtfoDta", Chain::DogecoinTestnet),
        ] {
            let parse_address = Address::parse(address, chain.clone()).unwrap();
            let script_pubkey = parse_address.script_pubkey().unwrap();
            let address_from_script = Address::from_script(&script_pubkey, chain).unwrap();
            let display_address = address_from_script.to_string();
            assert_eq!(display_address, address);
        }
    }

    #[test]
    fn test_parse_uppercase_bech32_address() {
        // BIP-173 allows all-uppercase Bech32 strings (commonly produced by QR encoders).
        // Regression: tx 5sUQNPbKjdrYEBJmJBX47ddaMHGWsizRynz1MHujG4RB panicked on this exact address
        // with "Bech32 HRP mismatch: expected 'bc', got 'BC'".
        let upper = "BC1QTGJGS0VZ4FFAEZ59Y64VYTJP6034RPEZGYH8JT";
        let lower = "bc1qtgjgs0vz4ffaez59y64vytjp6034rpezgyh8jt";

        let from_upper = Address::parse(upper, Chain::BitcoinMainnet).unwrap();
        let from_lower = Address::parse(lower, Chain::BitcoinMainnet).unwrap();
        assert_eq!(from_upper, from_lower);
        assert!(matches!(from_upper, Address::Segwit { .. }));
        assert_eq!(
            from_upper.script_pubkey().unwrap(),
            from_lower.script_pubkey().unwrap()
        );
    }

    #[test]
    fn test_parse_rejects_wrong_network_hrp() {
        // A testnet bech32 address must NOT decode under BitcoinMainnet, even with the
        // case-insensitive HRP fix. Guards against accidentally relaxing the network check.
        let testnet = "tb1pt34385rvqtyuz6muh9hr5ed4fy0cx89zz0faxm6dhku0vqp2pxxs0ymh7y";
        let err = Address::parse(testnet, Chain::BitcoinMainnet).unwrap_err();
        assert!(
            err.contains("HRP mismatch"),
            "expected HRP mismatch error, got: {err}"
        );
    }

    #[test]
    fn test_from_pubkey() {
        for chain in [
            Chain::BitcoinMainnet,
            Chain::BitcoinTestnet,
            Chain::LitecoinMainnet,
            Chain::LitecoinTestnet,
            Chain::ZcashMainnet,
            Chain::ZcashTestnet,
            Chain::DogecoinMainnet,
            Chain::DogecoinTestnet,
        ] {
            let btc_public_key = generate_btc_public_key("path");
            let address = Address::from_pubkey(chain.clone(), btc_public_key).unwrap();
            let script_pubkey = address.script_pubkey().unwrap();
            let address_from_script = Address::from_script(&script_pubkey, chain.clone()).unwrap();
            assert_eq!(address, address_from_script);
            let address_from_str = Address::parse(&address_from_script.to_string(), chain).unwrap();
            assert_eq!(address, address_from_str);
        }
    }

    #[cfg(feature = "zcash")]
    #[test]
    fn test_get_branch_id_activation_boundaries() {
        use zcash_protocol::consensus::BranchId;

        // Mainnet: NU6.1 at 3_146_400, NU6.2 at 3_364_600.
        assert_eq!(Chain::ZcashMainnet.get_branch_id(3_146_399), BranchId::Nu6);
        assert_eq!(
            Chain::ZcashMainnet.get_branch_id(3_146_400),
            BranchId::Nu6_1
        );
        assert_eq!(
            Chain::ZcashMainnet.get_branch_id(3_364_599),
            BranchId::Nu6_1
        );
        assert_eq!(
            Chain::ZcashMainnet.get_branch_id(3_364_600),
            BranchId::Nu6_2
        );

        // Testnet: NU6.1 at 3_536_500, NU6.2 at 4_052_000.
        assert_eq!(Chain::ZcashTestnet.get_branch_id(3_536_499), BranchId::Nu6);
        assert_eq!(
            Chain::ZcashTestnet.get_branch_id(3_536_500),
            BranchId::Nu6_1
        );
        assert_eq!(
            Chain::ZcashTestnet.get_branch_id(4_051_999),
            BranchId::Nu6_1
        );
        assert_eq!(
            Chain::ZcashTestnet.get_branch_id(4_052_000),
            BranchId::Nu6_2
        );
    }
}
