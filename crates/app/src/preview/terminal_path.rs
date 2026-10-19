use super::resolve_preview_path;
use std::path::{Path, PathBuf};
use terminal::parse_path_ref;

/// 终端路径解析结果；`line`/`col` 来自 `path:行:列` 后缀。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TerminalPathHit {
    pub path: PathBuf,
    pub line: Option<u32>,
    pub col: Option<u32>,
}

/// 只在后台调用：按 `bases` 顺序（Agent worktree、项目根）验证有界文本候选，
/// 不扫描目录，也不按文件名模糊查找。第一个有命中的 base 决定结果。
pub(crate) fn resolve_terminal_path(
    bases: &[PathBuf],
    home: Option<&Path>,
    targets: &[String],
) -> Result<TerminalPathHit, String> {
    for base in bases {
        if let Some(hit) = resolve_in_base(base, home, targets)? {
            return Ok(hit);
        }
    }
    Err(format!("File does not exist: {}", targets.join(" | ")))
}

fn resolve_in_base(
    base: &Path,
    home: Option<&Path>,
    targets: &[String],
) -> Result<Option<TerminalPathHit>, String> {
    let mut found: Option<(PathBuf, TerminalPathHit)> = None;
    for target in targets {
        let (text, line, col) = match parse_path_ref(target) {
            Some(path_ref) => (path_ref.path, path_ref.line, path_ref.col),
            None => (target.clone(), None, None),
        };
        let text = expand_home(&text, home);
        let path = resolve_preview_path(base, &text).map_err(|error| error.to_string())?;
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Could not inspect {}: {error}", path.display())),
        };
        if !metadata.is_file() && !metadata.is_dir() {
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
            Some(_) => {}
            None => found = Some((identity, TerminalPathHit { path, line, col })),
        }
    }
    Ok(found.map(|(_, hit)| hit))
}

fn expand_home(text: &str, home: Option<&Path>) -> String {
    match (text.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => text.to_string(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/terminal_preview_tests.rs"]
mod tests;
