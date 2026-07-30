use crate::app::MultisoulApp;
use canvas::{BrowserSurfaceKind, CanvasNodeKind, EgoCardPhase, NodeId};
use gpui::Context;
use runtime::EgoTaskCommand;

/// Ego 外部窗口卡片的稳定 E2E 投影，不泄漏 controller 或渲染内部实现。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EgoCardE2eSnapshot {
    pub node_ids: Vec<NodeId>,
    pub task_space_ids: Vec<Option<String>>,
    pub phases: Vec<&'static str>,
    pub master_id: Option<NodeId>,
}

impl MultisoulApp {
    pub fn set_next_ego_task_space_for_e2e(&mut self, task_space_id: &str, task_space_name: &str) {
        self.ego.set_next_fake_task_space_result(
            task_space_id.to_string(),
            task_space_name.to_string(),
        );
    }

    pub fn set_ego_preview_for_e2e(&mut self, node_id: NodeId, cx: &mut Context<Self>) {
        let rgba = image::RgbaImage::from_pixel(4, 4, image::Rgba([0x12, 0x34, 0x56, 0xff]));
        let frame = crate::app::ego_preview::render_ego_preview_from_rgba(rgba);
        if let Some(previous) = self.ego_previews.replace_frame(node_id, frame) {
            cx.defer(move |cx| cx.drop_image(previous, None));
        }
        cx.notify();
    }

    pub fn ego_card_e2e_snapshot(&self) -> EgoCardE2eSnapshot {
        let mut node_ids = Vec::new();
        let mut task_space_ids = Vec::new();
        let mut phases = Vec::new();
        for node in self.runtime.document.nodes() {
            let CanvasNodeKind::Browser(spec) = &node.kind else {
                continue;
            };
            let BrowserSurfaceKind::EgoLite(ego) = &spec.surface else {
                continue;
            };
            node_ids.push(node.id);
            task_space_ids.push(ego.task_space_id.clone());
            phases.push(
                self.runtime
                    .ego_task_state(node.id)
                    .map(|state| ego_phase_name(state.phase))
                    .unwrap_or("missing"),
            );
        }
        EgoCardE2eSnapshot {
            node_ids,
            task_space_ids,
            phases,
            master_id: self.master_layout.master_id(),
        }
    }

    pub fn set_ego_agent_working_for_e2e(
        &mut self,
        node_id: NodeId,
        agent_node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.runtime
            .apply_ego_task_command(node_id, EgoTaskCommand::PrepareAgent { agent_node_id })
            .map_err(|error| error.to_string())?;
        self.runtime
            .apply_ego_task_command(node_id, EgoTaskCommand::AgentRunning { agent_node_id })
            .map_err(|error| error.to_string())?;
        self.mark_canvas_dirty();
        cx.notify();
        Ok(())
    }
}

fn ego_phase_name(phase: EgoCardPhase) -> &'static str {
    match phase {
        EgoCardPhase::Creating => "creating",
        EgoCardPhase::ReadyForUser => "ready_for_user",
        EgoCardPhase::PreparingAgent => "preparing_agent",
        EgoCardPhase::AgentWorking => "agent_working",
        EgoCardPhase::AwaitingUser => "awaiting_user",
        EgoCardPhase::UserControlling => "user_controlling",
        EgoCardPhase::ResumeRequested => "resume_requested",
        EgoCardPhase::Completed => "completed",
        EgoCardPhase::Failed => "failed",
        EgoCardPhase::Disconnected => "disconnected",
    }
}
