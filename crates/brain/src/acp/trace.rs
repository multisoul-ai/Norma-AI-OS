//! ACP 过程的观察口：把 `session/update` 整理成可展示的条目。纯数据与纯函数。

use serde_json::Value;
use std::time::SystemTime;

/// 不认识的更新类型原样记录时，正文最多保留的字数。
pub const RAW_LIMIT: usize = 2000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceKind {
    /// 发给大脑的提示全文。
    Prompt,
    Reply,
    Thought,
    ToolCall,
    ToolResult,
    Plan,
    /// 权限请求与闸门的判断。
    Permission,
    /// 一轮的耗时与停止原因。
    Turn,
    Error,
    /// 会话启动、重建等。
    Lifecycle,
    /// 没见过的更新类型，原样保留。
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceItem {
    pub kind: TraceKind,
    pub title: String,
    pub text: String,
}

impl TraceItem {
    pub fn new(kind: TraceKind, title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            text: text.into(),
        }
    }
}

/// 送到 App 的一条记录：哪条 lane、第几轮、什么时候、内容。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpTrace {
    pub at: SystemTime,
    pub lane: String,
    pub turn: u64,
    pub item: TraceItem,
}

fn cap(text: String) -> String {
    if text.chars().count() <= RAW_LIMIT {
        return text;
    }
    let head: String = text.chars().take(RAW_LIMIT).collect();
    format!("{head}…")
}

fn text_of(value: &Value) -> String {
    value["text"].as_str().unwrap_or_default().to_string()
}

fn tool_output(update: &Value) -> String {
    let from_content: Vec<String> = update["content"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let inner = if item["content"].is_object() {
                        &item["content"]
                    } else {
                        item
                    };
                    text_of(inner)
                })
                .filter(|text| !text.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !from_content.is_empty() {
        return from_content.join("\n");
    }
    match &update["rawOutput"] {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// 把一条 `session/update` 的 `update` 对象分类。没有类型字段的返回 `None`。
pub fn classify_update(update: &Value) -> Option<TraceItem> {
    let kind = update["sessionUpdate"].as_str()?;
    Some(match kind {
        "agent_message_chunk" => TraceItem::new(TraceKind::Reply, "", text_of(&update["content"])),
        "agent_thought_chunk" => {
            TraceItem::new(TraceKind::Thought, "", text_of(&update["content"]))
        }
        "tool_call" => {
            let input = match &update["rawInput"] {
                Value::Null => String::new(),
                other => other.to_string(),
            };
            TraceItem::new(
                TraceKind::ToolCall,
                update["title"].as_str().unwrap_or("工具调用"),
                input,
            )
        }
        "tool_call_update" => TraceItem::new(
            TraceKind::ToolResult,
            format!(
                "{} {}",
                update["toolCallId"].as_str().unwrap_or("工具"),
                update["status"].as_str().unwrap_or("更新")
            ),
            tool_output(update),
        ),
        "plan" => {
            let lines: Vec<String> = update["entries"]
                .as_array()
                .map(|entries| {
                    entries
                        .iter()
                        .map(|entry| {
                            format!(
                                "[{}] {}",
                                entry["status"].as_str().unwrap_or("?"),
                                entry["content"].as_str().unwrap_or_default()
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            TraceItem::new(TraceKind::Plan, "计划", lines.join("\n"))
        }
        other => TraceItem::new(TraceKind::Unknown, other, cap(update.to_string())),
    })
}

/// 把相邻的流式片段合并：回复、思考片段拼成一条；同标题的未知状态更新（如 usage_update）只留最新。
/// 其他类型出现时先输出已攒的内容，保持顺序。
#[derive(Default)]
pub struct Merger {
    pending: Option<TraceItem>,
}

impl Merger {
    pub fn push(&mut self, item: TraceItem) -> Vec<TraceItem> {
        let text_chunk = matches!(item.kind, TraceKind::Reply | TraceKind::Thought);
        let status = item.kind == TraceKind::Unknown;
        if let Some(pending) = self.pending.as_mut().filter(|p| p.kind == item.kind) {
            if text_chunk {
                pending.text.push_str(&item.text);
                return Vec::new();
            }
            if status && pending.title == item.title {
                *pending = item;
                return Vec::new();
            }
        }
        let mut out: Vec<TraceItem> = self.pending.take().into_iter().collect();
        if text_chunk || status {
            self.pending = Some(item);
        } else {
            out.push(item);
        }
        out
    }

    pub fn flush(&mut self) -> Option<TraceItem> {
        self.pending.take()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/trace_tests.rs"]
mod trace_tests;
