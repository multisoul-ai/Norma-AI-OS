/// Why a locally stored license token failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseInvalidReason {
    Missing,
    BadFormat,
    BadSignature,
    Expired,
    DidMismatch,
    BadStatus,
}

/// Outcome of offline license evaluation for the current machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalLicenseStatus {
    Activated {
        license_id: Option<String>,
        exp: Option<i64>,
    },
    Invalid {
        reason: LicenseInvalidReason,
    },
}
