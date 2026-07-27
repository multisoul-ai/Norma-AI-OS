use canvas::NodeId;
use gpui::{Context, Window};
use norma::protocol::{AgentTaskAssignment, CardCommand};

use super::MultisoulApp;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct AgentQueueUiState {
    open_node_id: Option<NodeId>,
    selected_task_ref: Option<String>,
    adding_task: bool,
}

impl AgentQueueUiState {
    pub(crate) fn open_node_id(&self) -> Option<NodeId> {
        self.open_node_id
    }

    pub(crate) fn selected_task_ref(&self) -> Option<&str> {
        self.selected_task_ref.as_deref()
    }

    pub(crate) fn adding_task(&self) -> bool {
        self.adding_task
    }

    pub(crate) fn toggle(&mut self, node_id: NodeId) {
        if self.open_node_id == Some(node_id) {
            self.close();
            return;
        }
        self.open_node_id = Some(node_id);
        self.selected_task_ref = None;
        self.adding_task = false;
    }

    pub(crate) fn select_task(&mut self, node_id: NodeId, task_ref: String) {
        if self.open_node_id != Some(node_id) {
            return;
        }
        self.selected_task_ref = if self.selected_task_ref.as_deref() == Some(task_ref.as_str()) {
            None
        } else {
            Some(task_ref)
        };
    }

    pub(crate) fn begin_add_task(&mut self, node_id: NodeId) {
        if self.open_node_id == Some(node_id) {
            self.adding_task = true;
            self.selected_task_ref = None;
        }
    }

    pub(crate) fn finish_add_task(&mut self) {
        self.adding_task = false;
    }

    pub(crate) fn finish_withdraw(&mut self, task_ref: &str) {
        if self.selected_task_ref.as_deref() == Some(task_ref) {
            self.selected_task_ref = None;
        }
    }

    pub(crate) fn close(&mut self) {
        self.open_node_id = None;
        self.selected_task_ref = None;
        self.adding_task = false;
    }
}

impl MultisoulApp {
    pub(crate) fn add_agent_queue_task(
        &mut self,
        node_id: NodeId,
        task: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = task.trim();
        if task.is_empty() {
            self.workspace_state.error = Some("队列任务不能为空".to_string());
            cx.notify();
            return;
        }
        let Some(agent_name) = self.agent_name_for_queue_node(node_id) else {
            self.workspace_state.error = Some("未找到 Agent 卡片".to_string());
            cx.notify();
            return;
        };
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let result = super::agent_runtime::execute_high_level_agent_command(
            self,
            CardCommand::DispatchAgentTasks {
                assignments: vec![AgentTaskAssignment {
                    name: agent_name,
                    task: task.to_string(),
                }],
                force_untracked: false,
            },
            area,
            zoom,
            agent::AgentDispatchSource::manual(),
            None,
        );
        match result {
            Ok(_) => {
                self.workspace_state.error = None;
                self.agent_queue_ui.finish_add_task();
            }
            Err(error) => self.workspace_state.error = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(crate) fn withdraw_agent_queue_task(
        &mut self,
        node_id: NodeId,
        task_ref: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_waiting = self
            .agent_definition_id_for_queue_node(node_id)
            .and_then(|definition_id| {
                self.agent_task_coordinator
                    .as_ref()
                    .and_then(|coordinator| coordinator.queue_snapshot(definition_id))
            })
            .is_some_and(|snapshot| {
                snapshot
                    .waiting
                    .iter()
                    .any(|task| task.task_ref == task_ref)
            });
        if !is_waiting {
            self.workspace_state.error = Some("只能撤回仍在等待的任务".to_string());
            cx.notify();
            return;
        }
        let area = self.visible_canvas_rect(window);
        let zoom = self.runtime.document.viewport.zoom;
        let result = super::agent_runtime::execute_high_level_agent_command(
            self,
            CardCommand::AgentTaskCancel {
                task_ref: task_ref.clone(),
            },
            area,
            zoom,
            agent::AgentDispatchSource::manual(),
            None,
        );
        match result {
            Ok(_) => {
                self.workspace_state.error = None;
                self.agent_queue_ui.finish_withdraw(&task_ref);
            }
            Err(error) => self.workspace_state.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn agent_definition_id_for_queue_node(&self, node_id: NodeId) -> Option<i64> {
        let node = self.runtime.document.node(node_id)?;
        let canvas::CanvasNodeKind::Agent(spec) = &node.kind else {
            return None;
        };
        spec.definition_id
    }

    fn agent_name_for_queue_node(&self, node_id: NodeId) -> Option<String> {
        let definition_id = self.agent_definition_id_for_queue_node(node_id)?;
        self.agent_definitions
            .definitions()
            .iter()
            .find(|definition| definition.id == definition_id)
            .map(|definition| definition.name.clone())
    }
}
