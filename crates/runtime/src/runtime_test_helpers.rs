//! Runtime 测试专用 helper：把 agent spawn 走真实 `TermLive::spawn_command`
//! 路径，供跨测试模块复用。仅 `#[cfg(test)]` 编译。
//!
//! 作为 `runtime` 的子模块，可访问 `Runtime` 的私有字段与方法。

use crate::nodes::agent::agent_command;
use crate::nodes::term::TermLive;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, NodeId, NodeType};
use terminal::{PtyCommand, PtyError};

#[cfg(test)]
impl super::Runtime {
    /// 测试用：以指定 command spawn 一个 terminal-backed agent live 节点，
    /// 复用生产 `TermLive::spawn_command`（真实 PTY），便于事件/状态回归。
    pub fn spawn_agent_with_command(
        &mut self,
        profile: AgentProfile,
        bounds: CanvasRect,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.allocate_session_id();
        let live =
            TermLive::spawn_command(sid, command.with_cwd(self.workspace_root()), cols, rows)?;
        let node_id = self.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile,
                alias: None,
            }),
            bounds,
        );
        self.live.insert(node_id, Box::new(live));
        self.events.register(node_id, NodeType::Agent);
        Ok(node_id)
    }

    /// 测试用：profile → 默认（无 hook env）PtyCommand 速查。
    pub fn agent_command(profile: AgentProfile) -> PtyCommand {
        agent_command(profile, None)
    }
}
