//! 幕僚会话的工具门禁。主会话和审批会话不走这里。
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

/// 拆开 `rawInput` 后，命令必须是 `norma_bin brain ...`，且不能带 shell 复合语法。
pub fn staff_command_allowed(raw_input: &Value, norma_bin: &Path) -> bool {
    command_text(raw_input).is_some_and(|command| command_allowed(&command, norma_bin))
}

pub(super) fn command_text(raw_input: &Value) -> Option<String> {
    let command = if raw_input.get("command").is_some() {
        &raw_input["command"]
    } else {
        raw_input
    };
    command
        .as_str()
        .or_else(|| command["normaShellCommand"].as_str())
        .or_else(|| raw_input["normaShellCommand"].as_str())
        .map(str::to_owned)
}

pub(super) fn command_allowed(command: &str, norma_bin: &Path) -> bool {
    if !norma_bin.is_absolute() || has_metachar(command) {
        return false;
    }
    let Some(body) = peel_shell(command) else {
        return false;
    };
    if has_metachar(&body) {
        return false;
    }
    let Some(argv) = split_argv(&body) else {
        return false;
    };
    argv.first().is_some_and(|bin| Path::new(bin) == norma_bin)
        && argv.get(1).is_some_and(|arg| arg == "brain")
}

/// 项目外的 Read 拒绝。相对路径先按工作目录展开，`..` 先折掉再比较。
pub(super) fn read_inside_project(path: &str, cwd: &Path) -> bool {
    if path.is_empty() || path.contains('\0') {
        return false;
    }
    let joined = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        cwd.join(path)
    };
    let cwd = lexical(cwd);
    let full = lexical(&joined);
    full.starts_with(&cwd)
}

fn peel_shell(command: &str) -> Option<String> {
    let argv = split_argv(command)?;
    if argv.len() == 3 && is_zsh(&argv[0]) && (argv[1] == "-lc" || argv[1] == "-c") {
        return Some(argv[2].clone());
    }
    Some(command.to_owned())
}

fn is_zsh(bin: &str) -> bool {
    Path::new(bin).file_name().and_then(|name| name.to_str()) == Some("zsh")
}

fn has_metachar(text: &str) -> bool {
    text.contains("$(")
        || text
            .chars()
            .any(|c| matches!(c, ';' | '&' | '|' | '`' | '>' | '<'))
}

fn split_argv(input: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_arg = false;
    let mut quote = None;
    while let Some(ch) = chars.next() {
        if quote == Some(ch) {
            quote = None;
            in_arg = true;
            continue;
        }
        if quote == Some('"') && ch == '\\' {
            current.push(chars.next()?);
            in_arg = true;
            continue;
        }
        if quote.is_some() {
            current.push(ch);
            in_arg = true;
            continue;
        }
        if ch == '\\' {
            current.push(chars.next()?);
            in_arg = true;
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            in_arg = true;
            continue;
        }
        if ch.is_whitespace() {
            if in_arg {
                args.push(std::mem::take(&mut current));
                in_arg = false;
            }
            continue;
        }
        current.push(ch);
        in_arg = true;
    }
    if quote.is_some() {
        return None;
    }
    if in_arg {
        args.push(current);
    }
    Some(args)
}

fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/staff_shell_tests.rs"]
mod staff_shell_tests;
