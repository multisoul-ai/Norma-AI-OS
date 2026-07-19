use futures::future::BoxFuture;
use norma::protocol::AgentProfile;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

mod commands;
use commands::parse_adapter_command;

#[derive(Clone, Debug)]
pub struct ToolContext {
    pub project_id: String,
    pub turn_id: String,
    pub user_input: String,
    pub contextual_delete_targets: Vec<u64>,
    pub tool_call_id: String,
    pub operation_id: Option<String>,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterCommand {
    FindAgentTasks {
        task_id: Option<u64>,
        agent: Option<String>,
        states: Vec<String>,
    },
    CreateAgent {
        profile: AgentProfile,
    },
    AssignAgentTask {
        agent: String,
        task: String,
    },
    CancelAgentTask {
        task_id: u64,
    },
    CreateTerminal,
    CreateBrowser {
        url: Option<String>,
    },
    CreatePreview {
        path: PathBuf,
        explicit_type: Option<norma::protocol::PreviewType>,
    },
    FocusCard {
        node_id: u64,
    },
    SendTerminalInput {
        node_id: u64,
        text: String,
        submit: bool,
    },
    DeleteCanvasCard {
        node_id: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdapterResult {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Value>,
}

impl AdapterResult {
    pub fn accepted(message: impl Into<String>) -> Self {
        Self {
            status: "accepted".to_string(),
            message: message.into(),
            task_id: None,
            node_id: None,
            tasks: Vec::new(),
        }
    }

    pub fn succeeded(message: impl Into<String>) -> Self {
        Self {
            status: "succeeded".to_string(),
            message: message.into(),
            task_id: None,
            node_id: None,
            tasks: Vec::new(),
        }
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self {
            status: "unknown".to_string(),
            message: message.into(),
            task_id: None,
            node_id: None,
            tasks: Vec::new(),
        }
    }

    pub fn with_task_id(mut self, task_id: u64) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_node_id(mut self, node_id: u64) -> Self {
        self.node_id = Some(node_id);
        self
    }

    pub fn with_tasks(mut self, tasks: Vec<Value>) -> Self {
        self.tasks = tasks;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum AdapterError {
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
    #[error("tool execution cancelled")]
    Cancelled,
    #[error("adapter rejected operation: {0}")]
    Rejected(String),
    #[error("adapter execution failed: {0}")]
    Failed(String),
}

pub trait AgentCardAdapter: Send + Sync {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>>;
}

#[derive(Clone)]
pub struct NormaToolRuntime {
    adapter: Arc<dyn AgentCardAdapter>,
    operation_receipts: Arc<Mutex<HashMap<String, Result<AdapterResult, AdapterError>>>>,
}

impl NormaToolRuntime {
    pub fn new(adapter: Arc<dyn AgentCardAdapter>) -> Self {
        Self {
            adapter,
            operation_receipts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn execute(
        &self,
        tool_name: &str,
        arguments: Value,
        context: &ToolContext,
    ) -> Result<AdapterResult, AdapterError> {
        if context.cancellation.is_cancelled() {
            return Err(AdapterError::Cancelled);
        }
        if let Some(operation_id) = context.operation_id.as_deref() {
            if let Some(receipt) = self
                .operation_receipts
                .lock()
                .expect("operation receipt lock should not be poisoned")
                .get(operation_id)
                .cloned()
            {
                return receipt;
            }
        }

        let command = parse_adapter_command(tool_name, arguments)?;
        if let AdapterCommand::DeleteCanvasCard { node_id } = &command {
            if !has_explicit_delete_intent(
                &context.user_input,
                *node_id,
                &context.contextual_delete_targets,
            ) {
                return Err(AdapterError::Rejected(
                    "用户原始请求没有明确授权删除这张卡片".to_string(),
                ));
            }
        }
        let result = self.adapter.execute(command, context).await;
        if let Some(operation_id) = context.operation_id.as_deref() {
            self.operation_receipts
                .lock()
                .expect("operation receipt lock should not be poisoned")
                .insert(operation_id.to_string(), result.clone());
        }
        result
    }
}

fn has_explicit_delete_intent(
    input: &str,
    node_id: u64,
    contextual_delete_targets: &[u64],
) -> bool {
    let normalized = input.to_lowercase();
    let negated = [
        "不要删除",
        "不用删除",
        "无需删除",
        "别删除",
        "不删除",
        "不要删",
        "不用删",
        "别删",
        "不要移除",
        "不用移除",
        "无需移除",
        "别移除",
        "do not delete",
        "do not remove",
        "don't delete",
        "don't remove",
        "dont delete",
        "dont remove",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase));
    if negated {
        return false;
    }
    let has_delete_verb = ["删除", "删掉", "移除", "delete", "remove"]
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_exact_target = normalized
        .split(|character: char| !character.is_ascii_digit())
        .filter(|token| !token.is_empty())
        .any(|token| token.parse::<u64>() == Ok(node_id));
    let has_contextual_target = [
        "当前卡片",
        "这张卡片",
        "选中卡片",
        "当前选中的卡片",
        "focused card",
        "selected card",
        "this card",
    ]
    .iter()
    .any(|target| normalized.contains(target));
    let contextual_target_matches =
        has_contextual_target && contextual_delete_targets.contains(&node_id);
    has_delete_verb && (has_exact_target || contextual_target_matches)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub fn norma_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        definition(
            "find_agent_tasks",
            "查询 AgentCard 任务；无筛选条件时返回最近的未结束任务。",
            object_schema(
                json!({
                    "task_id": {"type": "integer", "minimum": 1},
                    "agent": {"type": "string", "minLength": 1},
                    "states": {
                        "type": "array",
                        "items": {"type": "string", "enum": ["queued", "running", "cancel_requested", "succeeded", "failed", "cancelled", "interrupted"]},
                        "uniqueItems": true
                    }
                }),
                &[],
            ),
        ),
        definition(
            "create_agent",
            "创建并立即打开一个内置类型的 AgentCard；仅在用户明确要求创建时使用。",
            object_schema(
                json!({
                    "profile": {"type": "string", "enum": ["codex", "claude", "opencode", "cursor", "grok"]}
                }),
                &["profile"],
            ),
        ),
        definition(
            "assign_agent_task",
            "立即把一个任务派发给一个 AgentCard；目标明确时直接执行，无需确认。",
            object_schema(
                json!({
                    "agent": {"type": "string", "minLength": 1},
                    "task": {"type": "string", "minLength": 1}
                }),
                &["agent", "task"],
            ),
        ),
        definition(
            "cancel_agent_task",
            "取消一个明确 task_id 的 AgentCard 任务。",
            object_schema(
                json!({"task_id": {"type": "integer", "minimum": 1}}),
                &["task_id"],
            ),
        ),
        definition(
            "create_terminal",
            "直接创建一张 Terminal 卡片。",
            object_schema(json!({}), &[]),
        ),
        definition(
            "create_browser",
            "创建一张浏览器标签页卡片；用户未指定网址时省略 url，使用产品默认首页。",
            object_schema(json!({"url": {"type": "string", "minLength": 1}}), &[]),
        ),
        definition(
            "create_preview",
            "创建一张只读文件 Preview 卡片（用户也可能称 REVIEW 卡片）；path 可使用当前工作区中的相对路径或绝对路径。",
            object_schema(
                json!({
                    "path": {"type": "string", "minLength": 1},
                    "preview_type": {
                        "type": "string",
                        "enum": ["markdown", "text", "image"]
                    }
                }),
                &["path"],
            ),
        ),
        definition(
            "focus_card",
            "聚焦一张已存在的 Canvas 卡片。",
            object_schema(
                json!({"node_id": {"type": "integer", "minimum": 1}}),
                &["node_id"],
            ),
        ),
        definition(
            "send_terminal_input",
            "向 Terminal 卡片发送文本；不能用于 AgentCard。",
            object_schema(
                json!({
                    "node_id": {"type": "integer", "minimum": 1},
                    "text": {"type": "string"},
                    "submit": {"type": "boolean"}
                }),
                &["node_id", "text", "submit"],
            ),
        ),
        definition(
            "delete_canvas_card",
            "仅在用户明确要求删除指定卡片时删除 Canvas 卡片。",
            object_schema(
                json!({"node_id": {"type": "integer", "minimum": 1}}),
                &["node_id"],
            ),
        ),
    ]
}

fn definition(name: &str, description: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}
