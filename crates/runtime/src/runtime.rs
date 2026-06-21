//! 节点生命周期 + LiveNode 注册表。把 canvas 文档与可运行节点组装在一起。

use crate::focus::{FocusState, InputMode};
use crate::live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeView};
#[cfg(test)]
use crate::nodes::agent::agent_command;
use crate::nodes::agent::spawn_agent_live;
use crate::nodes::term::TermLive;
use canvas::{
    AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, Document, NodeId, Selection, TermSpec,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use terminal::{PtyCommand, PtyError, TerminalSessionId};

pub struct Runtime {
    pub document: Document,
    pub selection: Selection,
    pub focus: FocusState,
    live: HashMap<NodeId, Box<dyn LiveNode>>,
    next_session_id: TerminalSessionId,
    workspace_root: Option<PathBuf>,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            document: Document::new(),
            selection: Selection::new(),
            focus: FocusState::new(),
            live: HashMap::new(),
            next_session_id: 1,
            workspace_root: None,
        }
    }

    pub fn new_for_workspace(root: impl Into<PathBuf>) -> Self {
        let mut runtime = Self::new();
        runtime.workspace_root = Some(root.into());
        runtime
    }

    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// 新建一个 Terminal 节点：spawn live runtime + 在画布加 Term 节点。返回画布节点 id。
    pub fn spawn_terminal(
        &mut self,
        bounds: CanvasRect,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.next_session_id;
        self.next_session_id += 1;
        let command = PtyCommand::new(shell).with_cwd(self.workspace_root());
        let live = TermLive::spawn_command(sid, command, cols, rows)?;
        let node_id = self
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), bounds);
        self.live.insert(node_id, Box::new(live));
        Ok(node_id)
    }

    pub fn spawn_agent(
        &mut self,
        profile: AgentProfile,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.next_session_id;
        self.next_session_id += 1;
        let live = spawn_agent_live(sid, profile, self.workspace_root(), cols, rows)?;
        let node_id = self
            .document
            .add_node(CanvasNodeKind::Agent(AgentSpec { profile }), bounds);
        self.live.insert(node_id, Box::new(live));
        Ok(node_id)
    }

    #[cfg(test)]
    fn spawn_agent_with_command(
        &mut self,
        profile: AgentProfile,
        bounds: CanvasRect,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.next_session_id;
        self.next_session_id += 1;
        let live =
            TermLive::spawn_command(sid, command.with_cwd(self.workspace_root()), cols, rows)?;
        let node_id = self
            .document
            .add_node(CanvasNodeKind::Agent(AgentSpec { profile }), bounds);
        self.live.insert(node_id, Box::new(live));
        Ok(node_id)
    }

    #[cfg(test)]
    fn agent_command(profile: AgentProfile) -> PtyCommand {
        agent_command(profile)
    }

    pub fn live_len(&self) -> usize {
        self.live.len()
    }

    pub fn node_caps(&self, node_id: NodeId) -> Caps {
        self.live
            .get(&node_id)
            .map(|node| node.caps())
            .unwrap_or(Caps::NONE)
    }

    pub fn node_view(&self, node_id: NodeId) -> Option<NodeView> {
        self.live.get(&node_id).and_then(|node| node.view())
    }

    pub fn resize_node(&mut self, node_id: NodeId, resize: NodeResize) -> Result<(), NodeError> {
        let Some(node) = self.live.get_mut(&node_id) else {
            return Err(NodeError::MissingNode);
        };
        node.resize(resize)
    }

    pub fn send_node_input(&mut self, node_id: NodeId, input: NodeInput) -> Result<(), NodeError> {
        let Some(node) = self.live.get_mut(&node_id) else {
            return Err(NodeError::MissingNode);
        };
        node.input(input)
    }

    /// drain 所有 live node；任一 dirty 则返回 true（app 据此决定是否 repaint）。
    pub fn drain_all(&mut self) -> bool {
        let mut dirty = false;
        for node in self.live.values_mut() {
            if node.drain() {
                dirty = true;
            }
        }
        dirty
    }

    /// 删除节点并清理与该节点绑定的 runtime 状态。
    pub fn remove_node(&mut self, node_id: NodeId) -> bool {
        if self.document.remove_node(node_id).is_none() {
            return false;
        }

        if let Some(mut node) = self.live.remove(&node_id) {
            node.kill();
        }

        self.selection.deselect(node_id);
        if matches!(self.focus.mode(), InputMode::NodeFocused(focused) if focused == node_id) {
            self.focus.exit_to_canvas();
        }

        true
    }

    /// 窗口关闭时杀掉所有子进程（SPEC §8）。
    pub fn kill_all(&mut self) {
        for node in self.live.values_mut() {
            node.kill();
        }
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "workspace_runtime_tests.rs"]
mod workspace_tests;
