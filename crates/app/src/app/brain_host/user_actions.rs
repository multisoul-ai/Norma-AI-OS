//! 用户在概览里对审批和大脑记录的操作，在拥有该卡片的项目会话里执行。

use crate::app::MultisoulApp;
use norma::agent_hook::ApprovalDecision;

#[cfg(test)]
#[path = "../../../tests/unit/brain_user_approval_tests.rs"]
mod tests;

impl MultisoulApp {
    /// 用户直接放行或拒绝一条审批；不经过大脑，也不要求大脑在运行。
    /// 返回原请求的异步回执，复用原生/受控会话各自的授权和审计路径。
    pub(crate) fn decide_approval_by_user(
        &mut self,
        node_id: u64,
        run_id: &str,
        request_id: &str,
        allow: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Result<async_channel::Receiver<Result<serde_json::Value, String>>, &'static str> {
        let decision = if allow {
            ApprovalDecision::Allow
        } else {
            ApprovalDecision::Deny
        };
        let target = self
            .runtime
            .mobile_approval_snapshots(node_id)?
            .into_iter()
            .find(|snapshot| {
                snapshot.target.run_id == run_id && snapshot.target.request_id == request_id
            })
            .ok_or("approval_changed")?
            .target;
        crate::norma::endpoint::mobile_approval::choose(self, &target, decision, cx)
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
