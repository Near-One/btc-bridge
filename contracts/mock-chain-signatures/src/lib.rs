use near_sdk::{env, near, Gas, PanicOnDefault, Promise, PromiseOrValue, PublicKey};

/// Mirrors `sign_call_gas_attachment_requirement_tera_gas` of the MPC contract
/// on mainnet and testnet.
const GAS_FOR_SIGN_CALL: Gas = Gas::from_tgas(15);

#[derive(PanicOnDefault)]
#[near(contract_state)]
pub struct Contract {
    pub public_key: PublicKey,
    /// `sign` fails for these derivation paths, to test failed MPC requests.
    pub fail_paths: Vec<String>,
}

#[near(serializers = [json])]
pub struct SignRequest {
    pub payload: [u8; 32],
    pub path: String,
    pub key_version: u32,
}

#[near(serializers = [json])]
pub struct BigR {
    pub affine_point: String,
}

#[near(serializers = [json])]
pub struct S {
    pub scalar: String,
}

#[near(serializers = [json])]
pub struct SignatureResponse {
    pub big_r: BigR,
    pub s: S,
    pub recovery_id: u8,
}

#[near(serializers=[json])]
pub struct DomainId(pub u64);

#[near]
impl Contract {
    #[init]
    pub fn new(public_key: PublicKey) -> Self {
        Self {
            public_key,
            fail_paths: Vec::new(),
        }
    }

    pub fn set_fail_paths(&mut self, paths: Vec<String>) {
        self.fail_paths = paths;
    }

    #[allow(unused_variables)]
    #[payable]
    pub fn sign(&mut self, request: SignRequest) -> Promise {
        assert!(
            env::prepaid_gas() >= GAS_FOR_SIGN_CALL,
            "Insufficient gas provided. Provided: {} Required: {}",
            env::prepaid_gas(),
            GAS_FOR_SIGN_CALL
        );
        // The MPC contract requires a deposit of at least 1 yoctoNEAR.
        assert!(
            !env::attached_deposit().is_zero(),
            "Attached deposit is lower than required"
        );
        assert!(
            !self.fail_paths.contains(&request.path),
            "Sign request failed"
        );
        env::log_str(&format!(
            "sign deposit {}",
            env::attached_deposit().as_yoctonear()
        ));
        Self::ext(env::current_account_id()).sign_helper(request.payload, request.path, 0)
    }

    /// Returns a mock signature whose `s` is `sha256(payload || path)`, so a
    /// test can check that each stored signature belongs to its own input.
    /// It is not a valid ECDSA signature.
    #[allow(unused_variables)]
    #[private]
    pub fn sign_helper(
        &mut self,
        payload: [u8; 32],
        path: String,
        depth: usize,
    ) -> PromiseOrValue<SignatureResponse> {
        let s = env::sha256([payload.as_slice(), path.as_bytes()].concat());
        PromiseOrValue::Value(SignatureResponse {
            big_r: BigR {
                affine_point: "025802983164945D1C3E40818FF569E275451CC33613EDDFA0E54D23710DFAF3C8"
                    .to_string(),
            },
            s: S {
                scalar: s.iter().map(|b| format!("{b:02X}")).collect(),
            },
            recovery_id: 1,
        })
    }

    #[allow(unused_variables)]
    pub fn public_key(&self, domain_id: Option<DomainId>) -> PublicKey {
        self.public_key.clone()
    }
}
