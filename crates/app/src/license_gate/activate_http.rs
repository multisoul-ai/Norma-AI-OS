//! Synchronous Keygate activate HTTP call (run on background_executor).

use license::{ActivateLicenseData, ActivateLicenseRequest};

#[derive(Debug, Clone)]
pub enum ActivateHttpError {
    Transport(String),
    Api { status: u16, body: String },
    Decode(String),
}

impl std::fmt::Display for ActivateHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(msg) => write!(f, "network error: {msg}"),
            Self::Api { status, body } => write!(f, "activate failed HTTP {status}: {body}"),
            Self::Decode(msg) => write!(f, "activate response decode error: {msg}"),
        }
    }
}

pub fn activate_license(
    base_url: &str,
    request: &ActivateLicenseRequest,
) -> Result<ActivateLicenseData, ActivateHttpError> {
    let url = format!("{}/api/v1/license/activate", base_url.trim_end_matches('/'));
    let key_fp = short_key_fingerprint(&request.license_key);
    let idem = format!("norma-activate-{}-{}", request.identifier, key_fp);

    let response = ureq::post(&url)
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", &idem)
        .send_json(request)
        .map_err(|err| ActivateHttpError::Transport(err.to_string()))?;

    let status = response.status();
    let body = response
        .into_body()
        .read_to_string()
        .map_err(|err| ActivateHttpError::Transport(err.to_string()))?;

    if status != 200 {
        return Err(ActivateHttpError::Api {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| ActivateHttpError::Decode(err.to_string()))?;
    if parsed.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return Err(ActivateHttpError::Api {
            status: status.as_u16(),
            body,
        });
    }
    let data = parsed
        .get("data")
        .cloned()
        .ok_or_else(|| ActivateHttpError::Decode("missing data".into()))?;
    let token = data
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ActivateHttpError::Decode("missing token".into()))?
        .to_string();
    let activate_status = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("activated")
        .to_string();
    let license_id = data
        .get("license_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(ActivateLicenseData {
        token,
        status: activate_status,
        license_id,
    })
}

fn short_key_fingerprint(license_key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    license_key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
