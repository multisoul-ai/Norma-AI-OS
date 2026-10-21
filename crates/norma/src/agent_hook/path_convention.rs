//! 文件路径输出约定：注入给 Agent，使回复里的路径可被 Norma 稳定解析并点击打开。
//!
//! 注入通道因 provider 而异（均为真机探针结论，见 docs/research/2026-10-20-agent-path-convention-injection.md）：
//! - Claude / Codex：`SessionStart` 的 `hookSpecificOutput.additionalContext`。
//! - Cursor：`sessionStart` 的顶层 `additional_context`（按官方文档实现，未能真机验证）。
//! - Kimi：`SessionStart` 的 stdout 不入上下文，只能用首条 `UserPromptSubmit` 的 stdout 文本。
//! - OpenCode / Pi：插件/扩展在 system prompt 钩子里追加，见各自模板。

use super::HookSource;
use serde_json::{json, Value};

pub const PATH_CONVENTION_PROMPT: &str = "\
[Norma file-path convention] When your reply mentions a file or directory, write it as an \
inline code span containing ONLY the path, optionally followed by :line or :line:col, e.g. \
`crates/app/src/main.rs:42`. Use a path relative to the project root (cwd) or an absolute \
path. One path per code span. Always include the directory and file extension. Never wrap a \
path in **bold** or markdown links, never put punctuation inside the span, and never split a \
path across lines. Norma turns these spans into clickable links that open in its file panel.";

/// 用户在“编排”设置里关闭注入时，Norma 以此环境变量值 `0` 启动 Agent。
pub const ENV_PATH_CONVENTION: &str = "NORMA_PATH_CONVENTION";

/// 环境里没有显式关闭（`NORMA_PATH_CONVENTION=0`）即视为开启。
pub fn path_convention_enabled_from_env() -> bool {
    std::env::var(ENV_PATH_CONVENTION).map_or(true, |value| value != "0")
}

/// 插件/扩展模板里用于嵌入约定文案的占位符（整个 JS 字符串字面量）。
pub(super) const PROMPT_PLACEHOLDER: &str = "__NORMA_PATH_CONVENTION__";

/// 把模板中的占位符替换为约定文案的 JSON 字符串字面量（同时是合法的 JS/TS 字符串）。
pub(super) fn embed_prompt(template: &str) -> String {
    let literal = serde_json::to_string(PATH_CONVENTION_PROMPT).unwrap_or_else(|_| "\"\"".into());
    template.replace(PROMPT_PLACEHOLDER, &literal)
}

/// 返回应写入 hook stdout 的内容；仅对 Norma 托管会话输出，其余一律 `None`，不污染 Agent。
///
/// `claim_once(session_id)` 仅对 Kimi 使用：首次针对某会话调用时返回 `true`，之后为 `false`，
/// 使约定只在会话第一条提示词注入一次。
pub fn session_context_output(
    source: HookSource,
    raw_json: &Value,
    norma_managed: bool,
    claim_once: &mut dyn FnMut(&str) -> bool,
) -> Option<String> {
    if !norma_managed {
        return None;
    }
    let event = raw_json.get("hook_event_name").and_then(Value::as_str)?;
    match (source, event) {
        (HookSource::Claude | HookSource::Codex, "SessionStart") => Some(
            json!({"hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": PATH_CONVENTION_PROMPT,
            }})
            .to_string(),
        ),
        (HookSource::Cursor, "sessionStart" | "SessionStart") => {
            Some(json!({"additional_context": PATH_CONVENTION_PROMPT}).to_string())
        }
        (HookSource::Kimi, "UserPromptSubmit") => {
            let session = raw_json.get("session_id").and_then(Value::as_str)?;
            claim_once(session).then(|| PATH_CONVENTION_PROMPT.to_string())
        }
        _ => None,
    }
}
