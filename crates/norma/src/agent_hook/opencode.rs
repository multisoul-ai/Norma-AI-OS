use super::{atomic_file, AgentHookError, OPENCODE_PLUGIN_MARKER};
use std::fs;
use std::path::Path;

pub(super) fn install_opencode_plugin(path: &Path) -> Result<(), AgentHookError> {
    atomic_file::replace(path, super::install::opencode_plugin_source().as_bytes())?;
    Ok(())
}

pub(super) fn uninstall_opencode_plugin(path: &Path) -> Result<(), AgentHookError> {
    if !path.exists() {
        return Ok(());
    }
    let source = fs::read_to_string(path)?;
    if source.contains(OPENCODE_PLUGIN_MARKER) {
        fs::remove_file(path)?;
    }
    Ok(())
}
