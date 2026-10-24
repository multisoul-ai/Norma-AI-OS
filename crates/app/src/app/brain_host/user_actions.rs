//! 用户在概览里对审批和大脑记录的操作，在拥有该卡片的项目会话里执行。

use crate::app::MultisoulApp;
use canvas::CanvasNodeKind;
use norma::agent_hook::ApprovalDecision;

impl MultisoulApp {
    /// 用户直接放行或拒绝一条审批；不经过大脑，也不要求大脑在运行。
    /// 返回是否真的生效（审批可能已被大脑或终端处理）。
    pub(crate) fn decide_approval_by_user(
        &mut self,
        node_id: u64,
        run_id: &str,
        request_id: &str,
        allow: bool,
    ) -> bool {
        let decision = if allow {
            ApprovalDecision::Allow
        } else {
            ApprovalDecision::Deny
        };
        let profile = self
            .runtime
            .document
            .node(node_id)
            .and_then(|node| match &node.kind {
                CanvasNodeKind::Agent(spec) => Some(spec.profile),
                _ => None,
            });
        match profile {
            Some(canvas::AgentProfile::Claude) => self
                .runtime
                .decide_mobile_approval(node_id, run_id, request_id, decision),
            Some(canvas::AgentProfile::Codex) => self
                .runtime
                .decide_mobile_codex_approval(node_id, run_id, request_id, decision)
                .is_ok(),
            _ => false,
        }
    }

    /// “停止该 Agent”：向卡片发 Ctrl-C，与取消任务时的做法一致。
    pub(crate) fn interrupt_agent_by_user(&mut self, node_id: u64) -> bool {
        let interrupt = runtime::NodeInput::Key(terminal::KeyInput {
            key: terminal::Key::Char('c'),
            ctrl: true,
            alt: false,
            shift: false,
        });
        self.runtime.send_node_input(node_id, interrupt).is_ok()
    }
}
