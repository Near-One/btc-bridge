use near_sdk::json_types::Base64VecU8;
use near_sdk::near;

#[near(serializers = [json])]
pub struct ChainSpecificData {
    pub orchard_bundle_bytes: Base64VecU8,
    pub expiry_height: u32,
}
