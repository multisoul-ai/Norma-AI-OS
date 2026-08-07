use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;

pub fn generate_signing_key() -> SigningKey {
    SigningKey::generate(&mut OsRng)
}

pub fn public_key_hex(signing_key: &SigningKey) -> String {
    hex::encode(signing_key.verifying_key().as_bytes())
}

/// Build a Keygate-style token: `base64url(json).base64url(signature)`.
/// The signed message is the base64url payload string bytes, not raw JSON.
pub fn sign_token_json(json: &str, signing_key: &SigningKey) -> String {
    let payload_b64 = URL_SAFE_NO_PAD.encode(json.as_bytes());
    let signature = signing_key.sign(payload_b64.as_bytes());
    format!(
        "{}.{}",
        payload_b64,
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    )
}
