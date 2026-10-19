use crate::{
    norma_tool_definitions, AdapterError, AdapterResult, NormaToolRuntime, ToolCallPolicy,
    ToolContext,
};
use rig_core::tool::{Tool, ToolCallExtensions};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

mod gpt_live;
pub(crate) use gpt_live::GptLiveDeleteCanvasCardTool;

#[derive(Default)]
struct CallIdentities {
    pending: VecDeque<String>,
    provider_ids: HashMap<String, String>,
    steps: HashMap<String, usize>,
}

#[derive(Clone)]
pub(crate) struct RigToolContext {
    base: ToolContext,
    identities: Arc<Mutex<CallIdentities>>,
    policy: ToolCallPolicy,
}

impl RigToolContext {
    pub(crate) fn new(base: ToolContext) -> Self {
        Self {
            base,
            identities: Arc::new(Mutex::new(CallIdentities::default())),
            policy: ToolCallPolicy::default(),
        }
    }

    pub(crate) fn context_project_is_personal(&self) -> bool {
        self.base.project_id.is_empty()
            && self.base.input_source != crate::AgentInputSource::MobileExecution
    }

    pub(crate) fn is_mobile_workspace(&self) -> bool {
        matches!(
            self.base.input_source,
            crate::AgentInputSource::MobileDiscussion | crate::AgentInputSource::MobileExecution
        )
    }

    pub(crate) fn is_gpt_live(&self) -> bool {
        self.base.input_source == crate::AgentInputSource::GptLiveVoice
    }

    pub(crate) fn unknown_seen(&self) -> bool {
        self.policy.unknown_seen()
    }

    pub(crate) fn failure_budget_exhausted(&self) -> bool {
        self.policy.failure_budget_exhausted()
    }

    pub(crate) fn record_tool_call(
        &self,
        provider_tool_call_id: Option<&str>,
        internal_call_id: &str,
    ) {
        let mut ids = self.identities.lock().expect("Rig call identity lock");
        let stable_id = provider_tool_call_id
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .or_else(|| ids.provider_ids.get(internal_call_id).cloned())
            .unwrap_or_else(|| internal_call_id.to_string());
        ids.pending.push_back(stable_id);
    }

    pub(crate) fn record_streamed_call(&self, provider_id: &str, internal_id: &str) {
        // Ollama has no provider call ID: each parsed occurrence keeps its Rig identity.
        if provider_id.is_empty() {
            return;
        }
        self.identities
            .lock()
            .expect("Rig call identity lock")
            .provider_ids
            .insert(internal_id.to_owned(), provider_id.to_owned());
    }

    fn next(&self) -> Result<ToolContext, AdapterError> {
        let mut ids = self.identities.lock().expect("Rig call identity lock");
        let tool_call_id = ids
            .pending
            .pop_front()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                AdapterError::Failed("Rig tool-call hook did not provide a call ID".to_string())
            })?;
        let mut context = self.base.clone();
        context.tool_call_id = tool_call_id.clone();
        context.operation_id = Some(
            if context.input_source == crate::AgentInputSource::GptLiveVoice {
                let next = ids.steps.len() + 1;
                let step = ids.steps.entry(tool_call_id).or_insert(next);
                format!("{}:step-{step}", context.turn_id)
            } else {
                format!("{}:{tool_call_id}", context.turn_id)
            },
        );
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
        binding
            .policy
            .execute(&self.runtime, name, args, &context)
            .await
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

define_rig_tool!(MemoryListTool, "memory_list", 12);
define_rig_tool!(MemorySearchTool, "memory_search", 13);
define_rig_tool!(MemoryReadTool, "memory_read", 14);
define_rig_tool!(MemoryWriteTool, "memory_write", 15);

define_rig_tool!(WorkspaceMemoryTool, "workspace_memory", 16);
define_rig_tool!(WorkspaceSnapshotTool, "workspace_snapshot", 17);
define_rig_tool!(AssignWorkspaceTaskTool, "assign_workspace_task", 18);
define_rig_tool!(OpenWorkspaceArtifactTool, "open_workspace_artifact", 19);
define_rig_tool!(LocateWorkspaceAgentTool, "locate_workspace_agent", 20);
