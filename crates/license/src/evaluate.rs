use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;

use crate::types::{LicenseInvalidReason, LocalLicenseStatus};

pub fn evaluate_local_license(
    token: Option<&str>,
    public_key_hex: &str,
    machine_id: &str,
    now_unix: i64,
) -> LocalLicenseStatus {
    let token = match token.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value,
        None => {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::Missing,
            };
        }
    };

    let Some(dot_idx) = token.rfind('.') else {
        return LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadFormat,
        };
    };
    if dot_idx == 0 || dot_idx + 1 >= token.len() {
        return LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadFormat,
        };
    }

    let (payload_b64, signature_b64) = token.split_at(dot_idx);
    let signature_b64 = &signature_b64[1..];

    let public_key_bytes = match decode_public_key_hex(public_key_hex) {
        Ok(bytes) => bytes,
        Err(_) => {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::BadFormat,
            };
        }
    };

    let verifying_key = match VerifyingKey::from_bytes(&public_key_bytes) {
        Ok(key) => key,
        Err(_) => {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::BadFormat,
            };
        }
    };

    let signature_bytes = match URL_SAFE_NO_PAD.decode(signature_b64) {
        Ok(bytes) => bytes,
        Err(_) => {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::BadFormat,
            };
        }
    };

    let signature = match Signature::from_slice(&signature_bytes) {
        Ok(signature) => signature,
        Err(_) => {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::BadFormat,
            };
        }
    };

    if verifying_key
        .verify(payload_b64.as_bytes(), &signature)
        .is_err()
    {
        return LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadSignature,
        };
    }

    let payload_bytes = match URL_SAFE_NO_PAD.decode(payload_b64) {
        Ok(bytes) => bytes,
        Err(_) => {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::BadFormat,
            };
        }
    };

    let payload: Value = match serde_json::from_slice(&payload_bytes) {
        Ok(value) => value,
        Err(_) => {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::BadFormat,
            };
        }
    };

    if !payload.is_object() {
        return LocalLicenseStatus::Invalid {
            reason: LicenseInvalidReason::BadFormat,
        };
    }

    if let Some(exp) = payload.get("exp").and_then(Value::as_i64) {
        if exp > 0 && now_unix > exp {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::Expired,
            };
        }
    }

    if let Some(did) = payload.get("did").and_then(Value::as_str) {
        if did != machine_id {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::DidMismatch,
            };
        }
    }

    if let Some(status) = payload.get("sts").and_then(Value::as_str) {
        if status != "active" {
            return LocalLicenseStatus::Invalid {
                reason: LicenseInvalidReason::BadStatus,
            };
        }
    }

    LocalLicenseStatus::Activated {
        license_id: payload
            .get("lid")
            .and_then(Value::as_str)
            .map(str::to_owned),
        exp: payload.get("exp").and_then(Value::as_i64),
    }
}

fn decode_public_key_hex(public_key_hex: &str) -> Result<[u8; 32], ()> {
    let trimmed = public_key_hex.trim();
    if trimmed.len() != 64 {
        return Err(());
    }

    let bytes = hex::decode(trimmed).map_err(|_| ())?;
    bytes.try_into().map_err(|_| ())
}
