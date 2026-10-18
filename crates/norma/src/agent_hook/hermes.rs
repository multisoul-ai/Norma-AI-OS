//! Hermes Agent 目录插件的受管安装。
//!
//! 插件目录为 `$HERMES_HOME/plugins/norma-agent-hook/`，由 `plugin.yaml` 与带标记的
//! `__init__.py` 组成。只管理带标记的自有文件，遇到非自有内容拒绝覆盖。
//! 启用/停用走 `hermes plugins enable|disable`（实测约 3 秒），一律放后台线程，
//! 不得阻塞 GPUI 前台。

use super::{atomic_file, AgentHookError, HERMES_PLUGIN_MARKER, HERMES_PLUGIN_NAME};
use std::fs;
use std::path::Path;

const PLUGIN_INIT_FILE: &str = "__init__.py";
const PLUGIN_MANIFEST_FILE: &str = "plugin.yaml";

pub(super) fn hermes_plugin_init_source() -> &'static str {
    include_str!("hermes_plugin.py")
}

pub(super) fn hermes_plugin_manifest_source() -> &'static str {
    include_str!("hermes_plugin.yaml")
}

fn conflict(dir: &Path) -> AgentHookError {
    AgentHookError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "refusing to overwrite unmanaged Hermes plugin at {}",
            dir.display()
        ),
    ))
}

/// 目录存在但不是 Norma 受管（无标记）时返回 true。
fn is_unmanaged(dir: &Path) -> Result<bool, AgentHookError> {
    match fs::read_to_string(dir.join(PLUGIN_INIT_FILE)) {
        Ok(source) => Ok(!source.contains(HERMES_PLUGIN_MARKER)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match fs::read_dir(dir) {
            Ok(mut entries) => Ok(entries.next().is_some()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(AgentHookError::Io(error)),
        },
        Err(error) => Err(AgentHookError::Io(error)),
    }
}

pub(super) fn install_hermes_plugin(dir: &Path) -> Result<(), AgentHookError> {
    if is_unmanaged(dir)? {
        return Err(conflict(dir));
    }
    atomic_file::replace(
        &dir.join(PLUGIN_MANIFEST_FILE),
        hermes_plugin_manifest_source().as_bytes(),
    )?;
    atomic_file::replace(
        &dir.join(PLUGIN_INIT_FILE),
        hermes_plugin_init_source().as_bytes(),
    )?;
    Ok(())
}

/// 仅删除带标记的自有目录内容；非自有目录原样保留。
pub(super) fn uninstall_hermes_plugin(dir: &Path) -> Result<(), AgentHookError> {
    if !dir.exists() || is_unmanaged(dir)? {
        return Ok(());
    }
    for name in [PLUGIN_INIT_FILE, PLUGIN_MANIFEST_FILE] {
        match fs::remove_file(dir.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AgentHookError::Io(error)),
        }
    }
    // 目录里若还有 Hermes 生成的缓存（如 __pycache__），remove_dir 失败属正常，保留即可。
    let _ = fs::remove_dir_all(dir.join("__pycache__"));
    let _ = fs::remove_dir(dir);
    Ok(())
}

pub(super) fn hermes_plugin_installed_at(dir: &Path) -> bool {
    fs::read_to_string(dir.join(PLUGIN_INIT_FILE))
        .is_ok_and(|source| source.contains(HERMES_PLUGIN_MARKER))
}

fn run_hermes_plugins(action: &str) -> bool {
    std::process::Command::new("/bin/zsh")
        .args([
            "-lic",
            &format!("hermes plugins {action} {HERMES_PLUGIN_NAME}"),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// 后台启用插件；失败（未安装 hermes 等）时文件保留，仅状态灯不可用。
pub(super) fn spawn_enable_hermes_plugin() {
    std::thread::spawn(|| {
        run_hermes_plugins("enable");
    });
}

/// 后台先 disable 再删除自有目录，避免 config.yaml 残留指向已删除的插件。
pub(super) fn spawn_disable_and_remove_hermes_plugin(dir: std::path::PathBuf) {
    std::thread::spawn(move || {
        run_hermes_plugins("disable");
        let _ = uninstall_hermes_plugin(&dir);
    });
}
