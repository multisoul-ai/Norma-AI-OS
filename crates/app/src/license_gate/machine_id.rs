//! Hardware machine identifier used as Keygate activate `identifier` / token `did`.

/// Stable per-machine id for license binding.
///
/// On macOS uses `kern.uuid` (hardware-backed). Other platforms fall back to a
/// hostname-derived string so unit tests and non-mac builds still compile.
pub fn current_machine_id() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Some(id) = macos_kern_uuid() {
            return id;
        }
    }
    fallback_machine_id()
}

#[cfg(target_os = "macos")]
fn macos_kern_uuid() -> Option<String> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", "kern.uuid"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn fallback_machine_id() -> String {
    std::env::var("HOST")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| "unknown-machine".to_string())
}
