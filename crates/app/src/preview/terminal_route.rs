//! 终端点击路径的呈现分流：纯决策 + 一次后台文件探测，不碰 GPUI。

use super::{infer_preview_type, local_content};
use std::{io::Read, path::Path};
use workspace::content::MAX_TEXT_BYTES;

const SNIFF_BYTES: u64 = 8 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalPathRoute {
    /// 右抽屉只读预览标签（md / 图片 / 项目外 txt）。
    PreviewTab,
    /// 右抽屉文件工作台，可按行号定位。
    Workbench,
    /// 项目内目录：在工作台文件树中展开定位。
    Directory,
    System,
}

/// 交给系统应用时的方式：可直接启动的目标只在 Finder 中显示，避免点击终端文本就运行程序。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SystemOpen {
    Open,
    RevealInFinder,
}

const LAUNCHABLE_EXTENSIONS: &[&str] = &[
    "app",
    "command",
    "terminal",
    "tool",
    "pkg",
    "mpkg",
    "dmg",
    "jar",
    "workflow",
    "action",
    "scpt",
    "scptd",
    "applescript",
    "sh",
    "bash",
    "zsh",
    "csh",
    "ksh",
    "py",
    "pl",
    "rb",
    "bat",
    "cmd",
    "ps1",
    "exe",
    "msi",
    "xpc",
    "appex",
    "kext",
    "prefpane",
    "saver",
    "wdgt",
];

pub(crate) fn system_open_mode(path: &Path, is_dir: bool, unix_mode: Option<u32>) -> SystemOpen {
    let launchable_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            LAUNCHABLE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        });
    let executable_file = !is_dir && unix_mode.is_some_and(|mode| mode & 0o111 != 0);
    if launchable_extension || executable_file {
        SystemOpen::RevealInFinder
    } else {
        SystemOpen::Open
    }
}

/// 只在后台线程调用：读取元数据后决定系统打开方式；读不到时保守地只在 Finder 中显示。
pub(crate) fn probe_system_open(path: &Path) -> SystemOpen {
    let Ok(meta) = std::fs::metadata(path) else {
        return SystemOpen::RevealInFinder;
    };
    #[cfg(unix)]
    let unix_mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode())
    };
    #[cfg(not(unix))]
    let unix_mode = None;
    system_open_mode(path, meta.is_dir(), unix_mode)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PathFacts {
    pub is_dir: bool,
    pub size: u64,
    pub binary: bool,
}

pub(crate) fn inspect_path(path: &Path) -> Result<PathFacts, String> {
    let meta = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if meta.is_dir() {
        return Ok(PathFacts {
            is_dir: true,
            size: 0,
            binary: false,
        });
    }
    let mut head = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(SNIFF_BYTES).read_to_end(&mut head))
        .map_err(|error| error.to_string())?;
    Ok(PathFacts {
        is_dir: false,
        size: meta.len(),
        binary: head.contains(&0),
    })
}

/// 在已规范化的项目根里定位文件：返回 (area_id, 相对路径)。
/// 来源区域优先；否则取最长（最深）的包含根，避免嵌套项目被外层吞掉。
pub(crate) fn locate_in_areas(
    path: &Path,
    areas: &[(String, std::path::PathBuf)],
    preferred: Option<&str>,
) -> Option<(String, String)> {
    let relative = |root: &Path| {
        path.strip_prefix(root)
            .ok()
            .filter(|rel| !rel.as_os_str().is_empty())
            .map(|rel| rel.to_string_lossy().into_owned())
    };
    let pick = areas
        .iter()
        .find(|(id, root)| Some(id.as_str()) == preferred && relative(root).is_some())
        .or_else(|| {
            areas
                .iter()
                .filter(|(_, root)| relative(root).is_some())
                .max_by_key(|(_, root)| root.components().count())
        })?;
    relative(&pick.1).map(|rel| (pick.0.clone(), rel))
}

pub(crate) fn route_terminal_path(
    path: &Path,
    facts: &PathFacts,
    in_project: bool,
) -> TerminalPathRoute {
    if facts.is_dir {
        return if in_project {
            TerminalPathRoute::Directory
        } else {
            TerminalPathRoute::System
        };
    }
    let previewable = infer_preview_type(path, None).is_ok();
    let path_text = path.to_string_lossy();
    match local_content::kind(&path_text) {
        local_content::Kind::Markdown | local_content::Kind::Image => {
            if previewable {
                TerminalPathRoute::PreviewTab
            } else {
                TerminalPathRoute::System
            }
        }
        local_content::Kind::Text | local_content::Kind::Html => {
            if in_project && !facts.binary && facts.size <= MAX_TEXT_BYTES {
                TerminalPathRoute::Workbench
            } else if previewable {
                TerminalPathRoute::PreviewTab
            } else {
                TerminalPathRoute::System
            }
        }
        local_content::Kind::Video | local_content::Kind::Other => TerminalPathRoute::System,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/terminal_route_tests.rs"]
mod tests;
