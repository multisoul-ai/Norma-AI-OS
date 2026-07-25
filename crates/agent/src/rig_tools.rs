use crate::{norma_tool_definitions, AdapterError, AdapterResult, NormaToolRuntime, ToolContext};
use rig_core::tool::{Tool, ToolCallExtensions};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct RigToolContext {
    base: ToolContext,
    pending_tool_call_ids: Arc<Mutex<VecDeque<String>>>,
    tool_call_count: Arc<AtomicU64>,
    consecutive_failures: Arc<AtomicU64>,
    unknown_seen: Arc<AtomicBool>,
}

impl RigToolContext {
    pub(crate) fn new(base: ToolContext) -> Self {
        Self {
            base,
            pending_tool_call_ids: Arc::new(Mutex::new(VecDeque::new())),
            tool_call_count: Arc::new(AtomicU64::new(0)),
            consecutive_failures: Arc::new(AtomicU64::new(0)),
            unknown_seen: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn unknown_seen(&self) -> bool {
        self.unknown_seen.load(Ordering::Acquire)
    }

    pub(crate) fn failure_budget_exhausted(&self) -> bool {
        self.consecutive_failures.load(Ordering::Acquire) >= 3
    }

    pub(crate) fn record_tool_call(
        &self,
        provider_tool_call_id: Option<&str>,
        internal_call_id: &str,
    ) {
        let stable_id = provider_tool_call_id
            .unwrap_or(internal_call_id)
            .to_string();
        self.pending_tool_call_ids
            .lock()
            .expect("pending Rig tool-call ID lock should not be poisoned")
            .push_back(stable_id);
    }

    fn next(&self) -> Result<ToolContext, AdapterError> {
        let count = self.tool_call_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count > 20 {
            return Err(AdapterError::Rejected(
                "Norma 单回合工具调用预算已耗尽".to_string(),
            ));
        }
        let tool_call_id = self
            .pending_tool_call_ids
            .lock()
            .expect("pending Rig tool-call ID lock should not be poisoned")
            .pop_front()
            .ok_or_else(|| {
                AdapterError::Failed("Rig tool-call hook did not provide a call ID".to_string())
            })?;
        let mut context = self.base.clone();
        context.tool_call_id = tool_call_id.clone();
        context.operation_id = Some(format!("{}:{tool_call_id}", context.turn_id));
        Ok(context)
    }
}

#[derive(Clone)]
struct NormaRigTool {
    runtime: NormaToolRuntime,
    definition_index: usize,
}

impl NormaRigTool {
    fn new(runtime: NormaToolRuntime, definition_index: usize) -> Self {
        Self {
            runtime,
            definition_index,
        }
    }

    fn description(&self) -> String {
        norma_tool_definitions()[self.definition_index]
            .description
            .clone()
    }

    fn parameters(&self) -> Value {
        norma_tool_definitions()[self.definition_index]
            .parameters
            .clone()
    }

    async fn execute(
        &self,
        name: &str,
        args: Value,
        extensions: &ToolCallExtensions,
    ) -> Result<AdapterResult, AdapterError> {
        let binding = extensions.get::<RigToolContext>().ok_or_else(|| {
            AdapterError::Failed("Norma tool runtime context is missing".to_string())
        })?;
        let context = binding.next()?;
        let result = self.runtime.execute(name, args, &context).await;
        match &result {
            Ok(result) => {
                binding.consecutive_failures.store(0, Ordering::Release);
                if result.status == "unknown" {
                    binding.unknown_seen.store(true, Ordering::Release);
                    return Err(AdapterError::Failed(
                        "AgentCard Adapter 回执为 unknown，停止后续工具调用".to_string(),
                    ));
                }
            }
            Err(_) => {
                binding.consecutive_failures.fetch_add(1, Ordering::AcqRel);
            }
        }
        result
    }
}

macro_rules! define_rig_tool {
    ($type_name:ident, $tool_name:literal, $index:expr) => {
        #[derive(Clone)]
        pub(crate) struct $type_name(NormaRigTool);

        impl $type_name {
            pub(crate) fn new(runtime: NormaToolRuntime) -> Self {
                Self(NormaRigTool::new(runtime, $index))
            }
        }

        impl Tool for $type_name {
            const NAME: &'static str = $tool_name;
            type Error = AdapterError;
            type Args = Value;
            type Output = AdapterResult;

            fn description(&self) -> String {
                self.0.description()
            }

            fn parameters(&self) -> Value {
                self.0.parameters()
            }

            async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
                Err(AdapterError::Failed(
                    "Norma tool must run inside a NormaSession".to_string(),
                ))
            }

            async fn call_with_extensions(
                &self,
                args: Self::Args,
                extensions: &ToolCallExtensions,
            ) -> Result<Self::Output, Self::Error> {
                self.0.execute(Self::NAME, args, extensions).await
            }
        }
    };
}

define_rig_tool!(FindAgentTasksTool, "find_agent_tasks", 0);
define_rig_tool!(CreateAgentTool, "create_agent", 1);
define_rig_tool!(AssignAgentTaskTool, "assign_agent_task", 2);
define_rig_tool!(CancelAgentTaskTool, "cancel_agent_task", 3);
define_rig_tool!(ReplyAgentTool, "reply_agent", 4);
define_rig_tool!(CancelAgentReplyTool, "cancel_agent_reply", 5);
define_rig_tool!(CreateTerminalTool, "create_terminal", 6);
define_rig_tool!(CreateBrowserTool, "create_browser", 7);
define_rig_tool!(CreatePreviewTool, "create_preview", 8);
define_rig_tool!(FocusCardTool, "focus_card", 9);
define_rig_tool!(SendTerminalInputTool, "send_terminal_input", 10);
define_rig_tool!(DeleteCanvasCardTool, "delete_canvas_card", 11);
