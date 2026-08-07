//! Pure boot-phase decision for the hard license gate (secondary seam).

use license::{evaluate_local_license, LocalLicenseStatus};

/// Which root UI to show at process start (before constructing the workbench).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    Activation,
    Workbench,
}

/// Map rollout flag + skip flag + offline evaluation into a boot phase.
pub fn decide_boot_phase(
    licenses_enabled: bool,
    skip_license: bool,
    status: &LocalLicenseStatus,
) -> BootPhase {
    if !licenses_enabled || skip_license {
        return BootPhase::Workbench;
    }
    match status {
        LocalLicenseStatus::Activated { .. } => BootPhase::Workbench,
        LocalLicenseStatus::Invalid { .. } => BootPhase::Activation,
    }
}

/// Evaluate a local token (if any) for boot. Thin wrapper for callers that already
/// loaded the token string and machine/pubkey inputs.
pub fn evaluate_for_boot(
    token: Option<&str>,
    public_key_hex: &str,
    machine_id: &str,
    now_unix: i64,
) -> LocalLicenseStatus {
    evaluate_local_license(token, public_key_hex, machine_id, now_unix)
}
