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
mod definitions;
mod ideas;
mod task_handoff;
pub use definitions::norma_tool_definitions;
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

/// 想法的派发目标：已有智能体（稳定三元定位）或在指定项目区域新建。
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum IdeaTarget {
    Agent {
        workspace_id: String,
        area_id: String,
        node_id: u64,
    },
    NewAgent {
        workspace_id: String,
        area_id: String,
        profile: AgentProfile,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum AdapterCommand {
    HandoffWithContext {
        request: String,
        reference: crate::context::ContextRef,
    },
    UpdateWorkingState(Box<crate::context::ContextStateUpdate>),
    ReadSharedContext {
        reference: crate::context::ContextRef,
        offset: usize,
    },
    /// Host-only lifecycle check. No model tool parses this variant.
    ContextCheckpoint {
        scope: crate::context::ContextScope,
    },
    Memory(crate::MemoryCommand),
    WorkspaceMemory {
        area_id: String,
        command: crate::MemoryCommand,
    },
    WorkspaceSnapshot {
        scope: WorkspaceSnapshotScope,
        area_id: Option<String>,
    },
    AssignWorkspaceTask {
        workspace_id: String,
        area_id: String,
        node_id: u64,
        task: String,
    },
    /// 把需要读代码、整理或派后续任务的请求交给后台大脑，前台只先回一句。
    HandoffToBrain {
        request: String,
    },
    OpenWorkspaceArtifact {
        workspace_id: String,
        area_id: String,
        node_id: u64,
        turn_id: String,
        source_path: String,
        version: String,
    },
    LocateWorkspaceAgent {
        workspace_id: String,
        area_id: String,
        node_id: u64,
        approval_run_id: Option<String>,
        approval_turn_id: Option<String>,
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
    CaptureIdea {
        body: String,
    },
    ListIdeas,
    AssignIdea {
        idea_id: String,
        target: IdeaTarget,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceSnapshotScope {
    CurrentProject,
    Workspace,
    Area,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Value>,
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
            snapshot: None,
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
            snapshot: None,
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
            snapshot: None,
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

    pub fn with_snapshot(mut self, snapshot: Value) -> Self {
        self.snapshot = Some(snapshot);
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
    /// 任务文本已经提交给智能体，只是没拿到任务编号：不是失败，调用方不能回滚或重发，否则会重复派发。
    #[error("adapter accepted the task but returned no task id: {0}")]
    AcceptedWithoutTaskId(String),
    #[error("{0}")]
    Unknown(String),
}

pub trait AgentCardAdapter: Send + Sync {
    fn shared_context<'a>(
        &'a self,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<crate::context::ContextFrame>, AdapterError>> {
        Box::pin(async { Ok(None) })
    }
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

    pub async fn shared_context(
        &self,
        context: &ToolContext,
    ) -> Result<Option<crate::context::ContextFrame>, AdapterError> {
        self.adapter.shared_context(context).await
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

        let mut command = parse_adapter_command(tool_name, arguments)?;
        if context.input_source == crate::AgentInputSource::MobileDiscussion
            && !matches!(
                command,
                AdapterCommand::Memory(_)
                    | AdapterCommand::WorkspaceMemory { .. }
                    | AdapterCommand::FindAgentTasks { .. }
                    | AdapterCommand::ListIdeas
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
        task_handoff::preserve_doubao_request(&mut command, context);
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
