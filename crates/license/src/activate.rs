use serde::{Deserialize, Serialize};

/// Request body for `POST /license/activate` (Keygate spike contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateLicenseRequest {
    pub license_key: String,
    pub identifier: String,
    pub identifier_type: String,
    pub label: String,
}

/// Successful activation payload nested under `data`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateLicenseData {
    pub token: String,
    pub status: String,
    pub license_id: String,
}

/// Top-level activate API response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateLicenseResponse {
    pub success: bool,
    pub data: ActivateLicenseData,
}
