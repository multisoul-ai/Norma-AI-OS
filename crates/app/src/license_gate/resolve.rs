//! Resolve initial boot phase from env + local token file.

use super::boot::{decide_boot_phase, evaluate_for_boot, BootPhase};
use super::config::{public_key_hex, skip_license_enabled};
use super::machine_id::current_machine_id;
use super::token_store::{default_token_path, read_token};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn resolve_initial_boot_phase() -> BootPhase {
    if skip_license_enabled() {
        return BootPhase::Workbench;
    }
    let token = default_token_path().ok().and_then(|path| read_token(&path));
    let status = evaluate_for_boot(
        token.as_deref(),
        &public_key_hex(),
        &current_machine_id(),
        now_unix(),
    );
    decide_boot_phase(false, &status)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
