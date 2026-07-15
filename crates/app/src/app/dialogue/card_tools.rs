//! Card tool templates and synchronous ToolExecutor wiring for voice turns.

use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use crate::norma::server::{execute_card_command_for_area, NormaExecution};
use canvas::CanvasRect;
#[cfg(test)]
use dialogue::{DialogueError, ToolExecutor};
use norma::protocol::{AgentTaskReceiptStatus, CardCommand, CardCommandResult, NodeId};
use runtime::Runtime;

pub(crate) fn format_create_ok(node_id: NodeId) -> String {
    format!("已创建卡片 {node_id}")
}

pub(crate) fn format_send_text_ok(node_id: NodeId) -> String {
    format!("已发送到卡片 {node_id}")
}

pub(crate) fn format_focus_ok(node_id: NodeId) -> String {
    format!("已聚焦卡片 {node_id}")
}

pub(crate) fn format_operation_failed(message: &str) -> String {
    format!("操作失败：{message}")
}

pub(crate) fn template_report_for_error(message: &str) -> String {
    format_operation_failed(message)
}

pub(crate) fn template_report_for_result(result: &CardCommandResult) -> Option<String> {
    match result {
        CardCommandResult::Created { node_id } => Some(format_create_ok(*node_id)),
        CardCommandResult::Sent { node_id } => Some(format_send_text_ok(*node_id)),
        CardCommandResult::Focused { node_id } => Some(format_focus_ok(*node_id)),
        CardCommandResult::AgentCreated { name, .. } => Some(format!("已创建 Agent {name}")),
        CardCommandResult::AgentActivated { name, .. } => Some(format!("已激活 Agent {name}")),
        CardCommandResult::AgentTasksDispatched { receipts } => {
            let failed = receipts
                .iter()
                .filter(|receipt| receipt.status == AgentTaskReceiptStatus::Failed)
                .count();
            let accepted = receipts.len() - failed;
            match (accepted, failed) {
                (0, failed) => Some(format!("Agent 任务提交失败 {failed} 项")),
                (accepted, 0) => Some(format!("已提交或排队 {accepted} 项 Agent 任务")),
                (accepted, failed) => Some(format!(
                    "已提交或排队 {accepted} 项 Agent 任务，失败 {failed} 项"
                )),
            }
        }
        CardCommandResult::Deleted { .. }
        | CardCommandResult::Listed { .. }
        | CardCommandResult::AgentsListed { .. } => None,
    }
}

pub(crate) trait CardCommandRunner {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, String>;
}

pub(crate) struct CanvasInlineCardRunner<'a> {
    pub(crate) runtime: &'a mut Runtime,
    pub(crate) motion: &'a mut MotionStore,
    pub(crate) master_layout: &'a mut MasterLayoutState,
    pub(crate) area: CanvasRect,
    pub(crate) zoom: f32,
    pub(crate) executions: Vec<NormaExecution>,
}

impl CardCommandRunner for CanvasInlineCardRunner<'_> {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, String> {
        let execution = execute_card_command_for_area(
            self.runtime,
            self.motion,
            self.master_layout,
            command,
            self.area,
            self.zoom,
        )
        .map_err(|error| error.to_string())?;
        let result = execution.result.clone();
        self.executions.push(execution);
        Ok(result)
    }
}

impl CanvasInlineCardRunner<'_> {
    pub(crate) fn take_executions(&mut self) -> Vec<NormaExecution> {
        std::mem::take(&mut self.executions)
    }
}

#[cfg(test)]
pub(crate) struct AppCardToolExecutor<'a, R: CardCommandRunner + ?Sized> {
    runner: &'a mut R,
}

#[cfg(test)]
impl<'a, R: CardCommandRunner + ?Sized> AppCardToolExecutor<'a, R> {
    pub(crate) fn new(runner: &'a mut R) -> Self {
        Self { runner }
    }
}

#[cfg(test)]
impl<R: CardCommandRunner + ?Sized> ToolExecutor for AppCardToolExecutor<'_, R> {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, DialogueError> {
        self.runner.execute(command).map_err(DialogueError::Message)
    }
}

#[cfg(test)]
pub(crate) struct RecordingCardCommandRunner {
    recorded: Vec<CardCommand>,
    handler: Box<dyn FnMut(CardCommand) -> Result<CardCommandResult, String> + Send>,
}

#[cfg(test)]
impl RecordingCardCommandRunner {
    pub(crate) fn new(
        handler: impl FnMut(CardCommand) -> Result<CardCommandResult, String> + Send + 'static,
    ) -> Self {
        Self {
            recorded: Vec::new(),
            handler: Box::new(handler),
        }
    }

    pub(crate) fn recorded_commands(&self) -> &[CardCommand] {
        &self.recorded
    }
}

#[cfg(test)]
impl CardCommandRunner for RecordingCardCommandRunner {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, String> {
        self.recorded.push(command.clone());
        (self.handler)(command)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app_dialogue_card_tool_tests.rs"]
mod app_dialogue_card_tool_tests;
