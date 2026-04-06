use crate::chain_signature::{BigR, SignatureResponse, S};
use crate::{
    serde_json, MAX_BOOL_RESULT, MAX_FT_TRANSFER_CALL_RESULT, MAX_PUBLIC_KEY_RESULT,
    MAX_SIGNATURE_RESULT,
};
use near_sdk::json_types::U128;
use near_sdk::PublicKey;

#[test]
fn test_bool_result_size_fits_constant() {
    // `false` is the larger of the two bool JSON representations (5 bytes vs 4 for `true`)
    let serialized = serde_json::to_vec(&false).unwrap();
    assert!(
        serialized.len() <= MAX_BOOL_RESULT,
        "serialized bool ({} bytes) exceeds MAX_BOOL_RESULT ({})",
        serialized.len(),
        MAX_BOOL_RESULT,
    );
}

#[test]
fn test_ft_transfer_call_result_size_fits_constant() {
    // U128(u128::MAX) produces the longest possible decimal string representation
    let serialized = serde_json::to_vec(&U128(u128::MAX)).unwrap();
    assert!(
        serialized.len() <= MAX_FT_TRANSFER_CALL_RESULT,
        "serialized U128::MAX ({} bytes) exceeds MAX_FT_TRANSFER_CALL_RESULT ({})",
        serialized.len(),
        MAX_FT_TRANSFER_CALL_RESULT,
    );
}

#[test]
fn test_public_key_result_size_fits_constant() {
    // secp256k1 compressed public key (33 bytes raw, base58-encoded with curve prefix)
    let pk: PublicKey = "secp256k1:4NfTiv3UsGahebgTaHyD9vF8KYKMBnfd6kh94mK6xv8fGBiJB8TBtFMP5WWXz6B89Ac1fbpzPwAvoyQebemHFwx3"
        .parse()
        .unwrap();
    let serialized = serde_json::to_vec(&pk).unwrap();
    assert!(
        serialized.len() <= MAX_PUBLIC_KEY_RESULT,
        "serialized PublicKey ({} bytes) exceeds MAX_PUBLIC_KEY_RESULT ({})",
        serialized.len(),
        MAX_PUBLIC_KEY_RESULT,
    );
}

#[test]
fn test_signature_result_size_fits_constant() {
    // Realistic worst-case SignatureResponse with full-length hex strings
    let response = SignatureResponse {
        big_r: BigR {
            // Compressed secp256k1 point: 02/03 prefix + 64 hex digits = 66 chars
            affine_point: "025802983164945D1C3E40818FF569E275451CC33613EDDFA0E54D23710DFAF3C8"
                .to_string(),
        },
        s: S {
            // 256-bit scalar: 64 hex digits
            scalar: "07511DF9E947BC61F88011A3166AA0E60E2D45BFCACD61AD35DB4340941C84DE".to_string(),
        },
        recovery_id: 1,
    };
    let serialized = serde_json::to_vec(&response).unwrap();
    assert!(
        serialized.len() <= MAX_SIGNATURE_RESULT,
        "serialized SignatureResponse ({} bytes) exceeds MAX_SIGNATURE_RESULT ({})",
        serialized.len(),
        MAX_SIGNATURE_RESULT,
    );
}
