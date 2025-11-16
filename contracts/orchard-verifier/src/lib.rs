use near_sdk::json_types::U128;
use near_sdk::{env, near, require};
use std::io::Cursor;
use zcash_address::unified::{Container, Encoding};

#[derive(Default)]
#[near(contract_state)]
pub struct Contract {}

#[near]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self {}
    }

    pub fn verify_orchard_bundle(&self, bundle_hex: String) {
        env::log_str("verify_orchard_bundle: start");
        let bytes = hex::decode(bundle_hex).expect("hex");
        env::log_str(&format!(
            "verify_orchard_bundle: bundle_bytes={}",
            bytes.len()
        ));
        let mut cursor = Cursor::new(bytes);
        let bundle =
            zcash_primitives::transaction::components::orchard::read_v5_bundle(&mut cursor)
                .expect("read bundle")
                .expect("some bundle");

        //require!(bundle.actions().len() == 1, "single action only");

        env::log_str("verify_orchard_bundle: building VK");
        let vk = orchard::circuit::VerifyingKey::build();
        env::log_str("verify_orchard_bundle: built VK");
        let flags = *bundle.flags();
        let anchor = *bundle.anchor();
        let instances = bundle
            .actions()
            .iter()
            .map(|a| a.to_instance(flags, anchor))
            .collect::<Vec<_>>();
        env::log_str(&format!(
            "verify_orchard_bundle: instances_len={}",
            instances.len()
        ));
        let proof = bundle.authorization().proof();
        env::log_str("verify_orchard_bundle: calling proof.verify");
        proof.verify(&vk, &instances).expect("valid orchard proof");
        env::log_str("verify_orchard_bundle: verify done");
    }

    pub fn verify_orchard_bundle_with_policy(
        &self,
        bundle_hex: String,
        target_addr: String,
        chain: String,
        expected_total_outflow: U128,
        miner_fee: U128,
    ) {
        let bytes = hex::decode(bundle_hex).expect("hex");
        let mut cursor = std::io::Cursor::new(bytes);
        let bundle =
            zcash_primitives::transaction::components::orchard::read_v5_bundle(&mut cursor)
                .expect("read bundle")
                .expect("some bundle");
        require!(bundle.actions().len() == 1, "single action only");

        // Recover output via OVK = 00..00
        let (note, addr, _memo) = bundle
            .recover_output_with_ovk(0, &orchard::keys::OutgoingViewingKey::from([0u8; 32]))
            .expect("recover with OVK");
        let orchard_amount = note.value().inner() as u128;
        let recovered_raw = addr.to_raw_address_bytes();

        // Extract Orchard receiver from UA
        let (ua_net, ua) = zcash_address::unified::Address::decode(&target_addr)
            .expect("Invalid Zcash address encoding");
        let expected_net = match chain.as_str() {
            "ZcashMainnet" => zcash_protocol::consensus::NetworkType::Main,
            _ => zcash_protocol::consensus::NetworkType::Test,
        };
        require!(ua_net == expected_net, "Address network mismatch");
        let mut expected_raw: Option<[u8; 43]> = None;
        for recv in ua.items_as_parsed() {
            if let zcash_address::unified::Receiver::Orchard(bytes) = recv {
                expected_raw = Some(*bytes);
                break;
            }
        }
        let expected_raw = expected_raw.expect("Unified address missing Orchard receiver");
        require!(recovered_raw == expected_raw, "Orchard recipient mismatch");

        // Policy: orchard_amount + miner_fee == expected_total_outflow
        require!(
            orchard_amount + miner_fee.0 == expected_total_outflow.0,
            "Orchard+fee totals mismatch"
        );

        // Verify proof
        env::log_str("verify_orchard_bundle_with_policy: building VK");
        let vk = orchard::circuit::VerifyingKey::build();
        env::log_str("verify_orchard_bundle_with_policy: built VK");
        let flags = *bundle.flags();
        let anchor = *bundle.anchor();
        let instances = bundle
            .actions()
            .iter()
            .map(|a| a.to_instance(flags, anchor))
            .collect::<Vec<_>>();
        env::log_str(&format!(
            "verify_orchard_bundle_with_policy: instances_len={}",
            instances.len()
        ));
        let proof = bundle.authorization().proof();
        env::log_str("verify_orchard_bundle_with_policy: calling proof.verify");
        proof.verify(&vk, &instances).expect("valid orchard proof");
        env::log_str("verify_orchard_bundle_with_policy: verify done");
    }

    /// Build the Orchard VerifyingKey only (no proof verification). Useful for
    /// measuring gas cost of VK construction alone.
    pub fn build_vk_only(&self) {
        env::log_str("build_vk_only: start");
        let _vk = orchard::circuit::VerifyingKey::build();
        env::log_str("build_vk_only: built VK");
        // Drop immediately; we're just measuring build cost.
    }

    /// Parse bundle and build VK, derive instances, but do not call verify.
    pub fn parse_and_build_only(&self, bundle_hex: String) {
        env::log_str("parse_and_build_only: start");
        let bytes = hex::decode(bundle_hex).expect("hex");
        env::log_str(&format!(
            "parse_and_build_only: bundle_bytes={}",
            bytes.len()
        ));
        let mut cursor = Cursor::new(bytes);
        let bundle =
            zcash_primitives::transaction::components::orchard::read_v5_bundle(&mut cursor)
                .expect("read bundle")
                .expect("some bundle");
        env::log_str(&format!(
            "parse_and_build_only: actions_len={}",
            bundle.actions().len()
        ));
        env::log_str("parse_and_build_only: building VK");
        let vk = orchard::circuit::VerifyingKey::build();
        env::log_str("parse_and_build_only: built VK");
        let flags = *bundle.flags();
        let anchor = *bundle.anchor();
        let instances = bundle
            .actions()
            .iter()
            .map(|a| a.to_instance(flags, anchor))
            .collect::<Vec<_>>();
        env::log_str(&format!(
            "parse_and_build_only: instances_len={}",
            instances.len()
        ));
        let _ = vk; // keep for parity
    }
}
