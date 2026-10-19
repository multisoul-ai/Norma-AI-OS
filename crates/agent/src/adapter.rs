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
mod delete_intent;
use delete_intent::{
    has_explicit_agent_delete_intent, has_explicit_delete_intent,
    has_explicit_position_delete_intent,
};

#[derive(Clone, Debug)]
pub struct ToolContext {
    pub project_id: String,
    pub turn_id: String,
    pub user_input: String,
    pub input_source: crate::AgentInputSource,
    pub contextual_delete_targets: Vec<u64>,
    pub tool_call_id: String,
    pub operation_id: Option<String>,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum AdapterCommand {
    Memory(crate::MemoryCommand),
    WorkspaceMemory {
        area_id: String,
        command: crate::MemoryCommand,
    },
    FindAgentTasks {
        task_id: Option<u64>,
        agent: Option<String>,
        states: Vec<String>,
    },
    CreateAgent {
        profile: AgentProfile,
        task: Option<String>,
    },
    AssignAgentTask {
        agent: String,
        task: String,
    },
    CancelAgentTask {
        task_id: u64,
    },
    ReplyAgent {
        agent: String,
        message: String,
    },
    CancelAgentReply {
        reply_id: Option<u64>,
        agent: Option<String>,
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
    DeleteCanvasCardAtPosition {
        position: u64,
    },
    DeleteAgentCard {
        agent: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<crate::MemoryToolOutput>,
}

impl AdapterResult {
    pub fn accepted(message: impl Into<String>) -> Self {
        Self {
            status: "accepted".to_string(),
            message: message.into(),
            task_id: None,
            node_id: None,
            reply_id: None,
            reply_status: None,
            tasks: Vec::new(),
            memory: None,
        }
    }

    pub fn succeeded(message: impl Into<String>) -> Self {
        Self {
            status: "succeeded".to_string(),
            message: message.into(),
            task_id: None,
            node_id: None,
            reply_id: None,
            reply_status: None,
            tasks: Vec::new(),
            memory: None,
        }
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self {
            status: "unknown".to_string(),
            message: message.into(),
            task_id: None,
            node_id: None,
            reply_id: None,
            reply_status: None,
            tasks: Vec::new(),
            memory: None,
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

    pub fn with_reply_id(mut self, reply_id: u64) -> Self {
        self.reply_id = Some(reply_id);
        self
    }

    pub fn with_reply_status(mut self, reply_status: impl Into<String>) -> Self {
        self.reply_status = Some(reply_status.into());
        self
    }

    pub fn with_tasks(mut self, tasks: Vec<Value>) -> Self {
        self.tasks = tasks;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Error, Serialize, Deserialize)]
pub enum AdapterError {
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
    #[error("tool execution cancelled")]
    Cancelled,
    #[error("adapter rejected operation: {0}")]
    Rejected(String),
    #[error("adapter execution failed: {0}")]
    Failed(String),
    #[error("{0}")]
    Unknown(String),
}

pub trait AgentCardAdapter: Send + Sync {
    fn memory_context(
        &self,
    ) -> BoxFuture<'_, Result<Option<crate::MemorySessionContext>, AdapterError>> {
        Box::pin(async { Ok(None) })
    }
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>>;
}

#[derive(Clone)]
pub struct NormaToolRuntime {
    adapter: Arc<dyn AgentCardAdapter>,
    operation_receipts: Arc<Mutex<HashMap<String, OperationReceipt>>>,
}

#[derive(Clone)]
struct OperationReceipt {
    request: Value,
    result: Result<AdapterResult, AdapterError>,
}

impl NormaToolRuntime {
    pub fn new(adapter: Arc<dyn AgentCardAdapter>) -> Self {
        Self {
            adapter,
            operation_receipts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn memory_context(
        &self,
    ) -> Result<Option<crate::MemorySessionContext>, AdapterError> {
        self.adapter.memory_context().await
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
        let request = json!({"tool":tool_name,"arguments":arguments,
            "project":context.project_id,"turn":context.turn_id,"user":context.user_input,
            "source":format!("{:?}", context.input_source)});
        if let Some(operation_id) = context.operation_id.as_deref() {
            if let Some(receipt) = self
                .operation_receipts
                .lock()
                .expect("operation receipt lock should not be poisoned")
                .get(operation_id)
                .cloned()
            {
                if receipt.request != request {
                    return Err(AdapterError::Rejected(
                        "操作ID与原始请求不匹配，未再次执行".into(),
                    ));
                }
                return receipt.result;
            }
        }

        let command = parse_adapter_command(tool_name, arguments)?;
        if context.input_source == crate::AgentInputSource::MobileDiscussion
            && !matches!(
                command,
                AdapterCommand::Memory(_)
                    | AdapterCommand::WorkspaceMemory { .. }
                    | AdapterCommand::FindAgentTasks { .. }
            )
        {
            return Err(AdapterError::Rejected(
                "这条群聊消息仅供讨论，用户尚未要求执行".into(),
            ));
        }
        // GPT's model selects the operation; only its typed target is resolved locally.
        let delete_authorized = match &command {
            _ if context.input_source == crate::AgentInputSource::GptLiveVoice => None,
            AdapterCommand::DeleteCanvasCard { node_id } => Some(has_explicit_delete_intent(
                &context.user_input,
                *node_id,
                &context.contextual_delete_targets,
            )),
            AdapterCommand::DeleteCanvasCardAtPosition { position } => Some(
                has_explicit_position_delete_intent(&context.user_input, *position),
            ),
            AdapterCommand::DeleteAgentCard { agent } => {
                Some(has_explicit_agent_delete_intent(&context.user_input, agent))
            }
            _ => None,
        };
        if context.input_source != crate::AgentInputSource::DoubaoVoice
            && delete_authorized == Some(false)
        {
            return Err(AdapterError::Rejected(
                "用户原始请求没有明确授权删除这个卡片目标".to_string(),
            ));
        }
        let result = self.adapter.execute(command, context).await;
        if let Some(operation_id) = context.operation_id.as_deref() {
            self.operation_receipts
                .lock()
                .expect("operation receipt lock should not be poisoned")
                .insert(
                    operation_id.to_string(),
                    OperationReceipt {
                        request,
                        result: result.clone(),
                    },
                );
        }
        result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub fn norma_tool_definitions() -> Vec<ToolDefinition> {
    let mut tools = vec![
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
            "仅在用户明确要求新建某个内置类型的 Agent 时使用。若同时要求新 Agent 执行任务，把完整任务放入 task，由系统在创建后立即入队，不要再调用 assign_agent_task。若用户点名的 Agent 画布上已有卡片，不要调用本工具，应 reply_agent 直接对话。",
            object_schema(
                json!({
                    "profile": {"type": "string", "enum": ["codex", "claude", "opencode", "cursor", "grok"]},
                    "task": {"type": "string", "minLength": 1}
                }),
                &["profile"],
            ),
        ),
        definition(
            "assign_agent_task",
            "把任务派发给已启用的 Agent；画布上没有卡片时系统会自动打开唯一一张。目标明确时直接执行，无需确认；已有卡片且用户只是对话时用 reply_agent。",
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
            "reply_agent",
            "向画布上已有卡片的 Agent 发送会话消息；有卡即可对话，不要求先激活或新建，也不创建新任务。",
            object_schema(
                json!({
                    "agent": {"type": "string", "minLength": 1},
                    "message": {"type": "string", "minLength": 1}
                }),
                &["agent", "message"],
            ),
        ),
        definition(
            "cancel_agent_reply",
            "撤回尚未提交的会话回复；可按 reply_id 或 Agent 名称取消。",
            object_schema(
                json!({
                    "reply_id": {"type": "integer", "minimum": 1},
                    "agent": {"type": "string", "minLength": 1}
                }),
                &[],
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
            "仅在用户明确要求删除或关闭指定卡片时使用；目标必须且只能是稳定 node_id、用户所说的 1-based 画布位置，或 Agent 呼叫名/语音别名之一。",
            delete_canvas_card_schema(),
        ),
    ];
    tools.extend(crate::memory_access::definitions());
    tools.push(definition("workspace_memory", "只读指定工作空间项目区域的项目或工作树记忆。area_id 必须来自目录；不接受路径。action 为 list/search/read，需要正文时使用 read。",object_schema(json!({"area_id":{"type":"string"},"action":{"type":"string","enum":["list","search","read"]},"scope":{"type":"string","enum":["project","worktree"]},"id":{"type":"string"},"query":{"type":"string"},"offset":{"type":"integer","minimum":0}}),&["area_id","action","scope"])));
    tools
}

fn delete_canvas_card_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "node_id": {"type": "integer", "minimum": 1},
            "position": {"type": "integer", "minimum": 1},
            "agent": {"type": "string", "minLength": 1}
        },
        "oneOf": [
            {"required": ["node_id"]},
            {"required": ["position"]},
            {"required": ["agent"]}
        ],
        "additionalProperties": false
    })
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
