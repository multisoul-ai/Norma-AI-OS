use super::{atomic_file, AgentHookError, PI_EXTENSION_MARKER};
use std::fs;
use std::path::{Path, PathBuf};

const PI_EXTENSION_SOURCE: &str = include_str!("pi_extension.ts");

/// Pi extension directory file: `$PI_CODING_AGENT_DIR` when set and non-empty, else `~/.pi/agent`.
pub(super) fn pi_extension_path(home: &Path, agent_dir_env: Option<&str>) -> PathBuf {
    let agent_dir = match agent_dir_env.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => home.join(".pi").join("agent"),
    };
    agent_dir
        .join("extensions")
        .join(super::PI_EXTENSION_FILE_NAME)
}

pub(super) fn install_pi_extension(path: &Path) -> Result<(), AgentHookError> {
    match fs::read_to_string(path) {
        Ok(source) if !source.contains(PI_EXTENSION_MARKER) => {
            return Err(AgentHookError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "refusing to overwrite unmanaged Pi extension at {}",
                    path.display()
                ),
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AgentHookError::Io(error)),
    }
    atomic_file::replace(
        path,
        super::path_convention::embed_prompt(PI_EXTENSION_SOURCE).as_bytes(),
    )?;
    Ok(())
}

pub(super) fn uninstall_pi_extension(path: &Path) -> Result<(), AgentHookError> {
    if !path.exists() {
        return Ok(());
    }
    let source = fs::read_to_string(path)?;
    if source.contains(PI_EXTENSION_MARKER) {
        fs::remove_file(path)?;
    }
    Ok(())
}
