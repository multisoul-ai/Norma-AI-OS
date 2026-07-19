mod activate;
mod evaluate;
mod types;

pub use activate::{ActivateLicenseData, ActivateLicenseRequest, ActivateLicenseResponse};
pub use evaluate::evaluate_local_license;
pub use types::{LicenseInvalidReason, LocalLicenseStatus};

#[cfg(test)]
#[path = "../tests/unit/evaluate_tests.rs"]
mod evaluate_tests;
