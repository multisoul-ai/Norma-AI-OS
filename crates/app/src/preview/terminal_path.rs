use super::resolve_preview_path;
use std::path::{Path, PathBuf};

/// 只在后台调用：验证有界文本候选，不扫描目录，也不按文件名模糊查找。
pub(super) fn resolve_terminal_preview_path(
    root: &Path,
    targets: &[String],
) -> Result<PathBuf, String> {
    let mut found = None;
    for target in targets {
        let path = resolve_preview_path(root, target).map_err(|error| error.to_string())?;
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Could not inspect {}: {error}", path.display())),
        };
        if !metadata.is_file() {
            continue;
        }
        let identity = path.canonicalize().map_err(|error| error.to_string())?;
        match &found {
            Some((previous, _)) if previous != &identity => {
                return Err(
                    "File link is ambiguous; expand the terminal and click the complete path."
                        .into(),
                );
            }
            _ => found = Some((identity, path)),
        }
    }
    found
        .map(|(_, path)| path)
        .ok_or_else(|| format!("File does not exist: {}", targets.join(" | ")))
}

#[cfg(test)]
#[path = "../../tests/unit/terminal_preview_tests.rs"]
mod tests;
