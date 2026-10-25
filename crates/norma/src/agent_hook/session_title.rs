//! 读取各 CLI 自己生成的会话名：Claude 的 transcript `ai-title`、Codex 的 `session_index.jsonl`。
//! 只读本地文件，不调用任何模型；读不到返回 `None`，由调用方回退到首条提示词。

use super::AgentHookEvent;
use crate::protocol::AgentProfile;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// 只读文件尾部；shortcut: 超过该范围的更早标题不会被看到，但最新的标题总在尾部。
const TAIL_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 120;

/// 这些事件之后 CLI 才可能写入或更新会话名：会话（恢复）启动时运行时会清掉旧会话名，须重读；提交提示词、工具调用后（运行中途首次拿到标题）和一轮结束时。
pub fn should_refresh(event: &AgentHookEvent) -> bool {
    matches!(
        event.provider_event.as_str(),
        "SessionStart" | "UserPromptSubmit" | "PostToolUse" | "Stop"
    )
}

/// 按事件所属的 CLI 取会话名；其他后端暂无稳定来源，返回 `None`。
pub fn session_title(event: &AgentHookEvent) -> Option<String> {
    match event.profile {
        AgentProfile::Claude => {
            claude_ai_title(Path::new(event.raw_json.get("transcript_path")?.as_str()?))
        }
        AgentProfile::Codex => codex_thread_name(
            &codex_home()?.join("session_index.jsonl"),
            event.session_id.as_deref()?,
        ),
        _ => None,
    }
}

/// 只知道会话 id（打开项目恢复会话时还没有 hook 事件）时读会话名。
/// Claude 的 transcript 在 `~/.claude/projects/<编码后的路径>/<id>.jsonl`，逐个项目目录找。
pub fn session_title_by_id(profile: AgentProfile, session_id: &str) -> Option<String> {
    match profile {
        AgentProfile::Claude => claude_title_in(
            &dirs::home_dir()?.join(".claude").join("projects"),
            session_id,
        ),
        AgentProfile::Codex => {
            codex_thread_name(&codex_home()?.join("session_index.jsonl"), session_id)
        }
        _ => None,
    }
}

/// `projects` 下每个项目目录里找 `<id>.jsonl`，取其最后的 ai-title。
pub fn claude_title_in(projects: &Path, session_id: &str) -> Option<String> {
    if session_id.is_empty() || session_id.contains(['/', '\\']) || session_id.contains("..") {
        return None;
    }
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|dir| dir.path().join(format!("{session_id}.jsonl")))
        .find(|path| path.is_file())
        .and_then(|path| claude_ai_title(&path))
}

fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))
}

/// transcript jsonl 里最后一条 `{"type":"ai-title","aiTitle":…}`。
pub fn claude_ai_title(transcript: &Path) -> Option<String> {
    last_line_value(transcript, |value| {
        (value["type"] == "ai-title").then(|| clean(value["aiTitle"].as_str()?))?
    })
}

/// `session_index.jsonl` 里该会话最后一条 `{"id":…,"thread_name":…}`。
pub fn codex_thread_name(index: &Path, session_id: &str) -> Option<String> {
    last_line_value(index, |value| {
        (value["id"] == session_id).then(|| clean(value["thread_name"].as_str()?))?
    })
}

fn clean(raw: &str) -> Option<String> {
    let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!line.is_empty()).then(|| line.chars().take(MAX_TITLE_CHARS).collect())
}

/// 从文件尾部向前找第一条 `pick` 返回 `Some` 的 JSON 行。
fn last_line_value(path: &Path, pick: impl Fn(&Value) -> Option<String>) -> Option<String> {
    // hook 给的路径不可信：FIFO 等特殊文件 open 会永久阻塞后台线程。
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    if start > 0 {
        lines.next(); // 起点可能落在一行中间
    }
    lines
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|value| pick(&value))
}

#[cfg(test)]
#[path = "../../tests/unit/session_title_tests.rs"]
mod session_title_tests;
