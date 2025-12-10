use near_sdk::near;

#[near(serializers = [json])]
pub struct ChainSpecificData {
    pub orchard_bundle_bytes: Option<String>,
    pub expiry_height: Option<u32>,
}