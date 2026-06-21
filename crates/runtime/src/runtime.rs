//! 节点生命周期 + LiveNode 注册表。把 canvas 文档与可运行节点组装在一起。

use crate::focus::{FocusState, InputMode};
use crate::live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeView};
#[cfg(test)]
use crate::nodes::agent::agent_command;
use crate::nodes::agent::spawn_agent_live;
use crate::nodes::term::TermLive;
use canvas::{
    AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, Document, DocumentSnapshot, NodeId,
    Selection, TermSpec,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use storage::{CanvasStore, SessionCheckpoint, SessionCheckpointKind, SessionStore, StorageError};
use terminal::{PtyCommand, PtyError, TerminalSessionId};

pub const DEFAULT_TERMINAL_SHELL: &str = "/bin/zsh";
const DEFAULT_SESSION_COLS: u16 = 80;
const DEFAULT_SESSION_ROWS: u16 = 24;

pub struct Runtime {
    pub document: Document,
    pub selection: Selection,
    pub focus: FocusState,
    live: HashMap<NodeId, Box<dyn LiveNode>>,
    terminal_shells: HashMap<NodeId, String>,
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
            terminal_shells: HashMap::new(),
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

    pub fn collect_canvas_snapshot(&self) -> DocumentSnapshot {
        self.document.to_snapshot()
    }

    pub fn restore_canvas_snapshot(&mut self, snapshot: DocumentSnapshot) {
        self.kill_all();
        self.document = Document::from_snapshot(snapshot);
        self.selection.clear();
        self.focus.exit_to_canvas();
        self.live.clear();
        self.terminal_shells.clear();
        self.next_session_id = 1;
    }

    pub fn load_canvas_snapshot(&mut self, store: &CanvasStore) -> Result<bool, StorageError> {
        let Some(snapshot) = store.load_snapshot()? else {
            return Ok(false);
        };
        self.restore_canvas_snapshot(snapshot);
        Ok(true)
    }

    pub fn save_canvas_snapshot(&self, store: &CanvasStore) -> Result<(), StorageError> {
        store.save_snapshot(&self.collect_canvas_snapshot())
    }

    pub fn collect_session_checkpoints(&self) -> Vec<SessionCheckpoint> {
        self.document
            .nodes()
            .iter()
            .filter(|node| self.live.contains_key(&node.id))
            .filter_map(|node| {
                let kind = match node.kind {
                    CanvasNodeKind::Term(_) => SessionCheckpointKind::Terminal,
                    CanvasNodeKind::Agent(_) => SessionCheckpointKind::Agent,
                    CanvasNodeKind::Rect(_) => return None,
                };
                let (cols, rows) = self.live_grid_size(node.id);
                Some(SessionCheckpoint {
                    node_id: node.id,
                    kind,
                    payload: node.kind.display_title().to_string(),
                    cols,
                    rows,
                    shell: match node.kind {
                        CanvasNodeKind::Term(_) => Some(
                            self.terminal_shells
                                .get(&node.id)
                                .cloned()
                                .unwrap_or_else(|| DEFAULT_TERMINAL_SHELL.to_string()),
                        ),
                        CanvasNodeKind::Agent(_) | CanvasNodeKind::Rect(_) => None,
                    },
                    agent_profile: match node.kind {
                        CanvasNodeKind::Agent(spec) => Some(spec.profile),
                        CanvasNodeKind::Term(_) | CanvasNodeKind::Rect(_) => None,
                    },
                })
            })
            .collect()
    }

    pub fn save_session_checkpoints(&self, store: &SessionStore) -> Result<(), StorageError> {
        for checkpoint in self.collect_session_checkpoints() {
            store.save_checkpoint(&checkpoint)?;
        }
        Ok(())
    }

    pub fn restore_live_sessions(&mut self, checkpoints: &[SessionCheckpoint]) -> usize {
        let checkpoints_by_node_id: HashMap<NodeId, &SessionCheckpoint> = checkpoints
            .iter()
            .map(|checkpoint| (checkpoint.node_id, checkpoint))
            .collect();
        let nodes: Vec<(NodeId, CanvasNodeKind)> = self
            .document
            .nodes()
            .iter()
            .map(|node| (node.id, node.kind))
            .collect();

        let mut restored = 0;
        for (node_id, kind) in nodes {
            if self.live.contains_key(&node_id) {
                continue;
            }

            let checkpoint = checkpoints_by_node_id.get(&node_id).copied();
            let did_restore = match kind {
                CanvasNodeKind::Term(_) => {
                    let terminal_checkpoint = checkpoint
                        .filter(|checkpoint| checkpoint.kind == SessionCheckpointKind::Terminal);
                    let (cols, rows) = checkpoint_grid_size(terminal_checkpoint);
                    let shell = terminal_checkpoint
                        .and_then(|checkpoint| checkpoint.shell.as_deref())
                        .unwrap_or(DEFAULT_TERMINAL_SHELL);
                    self.attach_terminal_live(node_id, shell, cols, rows)
                }
                CanvasNodeKind::Agent(spec) => {
                    let agent_checkpoint = checkpoint
                        .filter(|checkpoint| checkpoint.kind == SessionCheckpointKind::Agent);
                    let (cols, rows) = checkpoint_grid_size(agent_checkpoint);
                    let profile = agent_checkpoint
                        .and_then(|checkpoint| checkpoint.agent_profile)
                        .unwrap_or(spec.profile);
                    self.attach_agent_live(node_id, profile, cols, rows)
                }
                CanvasNodeKind::Rect(_) => false,
            };

            if did_restore {
                restored += 1;
            }
        }

        restored
    }

    /// 新建一个 Terminal 节点：spawn live runtime + 在画布加 Term 节点。返回画布节点 id。
    pub fn spawn_terminal(
        &mut self,
        bounds: CanvasRect,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.allocate_session_id();
        let command = terminal_shell_command(shell).with_cwd(self.workspace_root());
        let live = TermLive::spawn_command(sid, command, cols, rows)?;
        let node_id = self
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), bounds);
        self.terminal_shells.insert(node_id, shell.to_string());
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
        let sid = self.allocate_session_id();
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
        let sid = self.allocate_session_id();
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
        self.terminal_shells.remove(&node_id);

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

    fn allocate_session_id(&mut self) -> TerminalSessionId {
        let sid = self.next_session_id;
        self.next_session_id += 1;
        sid
    }

    fn attach_terminal_live(&mut self, node_id: NodeId, shell: &str, cols: u16, rows: u16) -> bool {
        let sid = self.allocate_session_id();
        let command = terminal_shell_command(shell).with_cwd(self.workspace_root());
        match TermLive::spawn_command(sid, command, cols, rows) {
            Ok(live) => {
                self.terminal_shells.insert(node_id, shell.to_string());
                self.live.insert(node_id, Box::new(live));
                true
            }
            Err(error) => {
                tracing::warn!(
                    ?node_id,
                    %shell,
                    %error,
                    "failed to restore terminal live session"
                );
                false
            }
        }
    }

    fn attach_agent_live(
        &mut self,
        node_id: NodeId,
        profile: AgentProfile,
        cols: u16,
        rows: u16,
    ) -> bool {
        let sid = self.allocate_session_id();
        match spawn_agent_live(sid, profile, self.workspace_root(), cols, rows) {
            Ok(live) => {
                self.live.insert(node_id, Box::new(live));
                true
            }
            Err(error) => {
                tracing::warn!(
                    ?node_id,
                    ?profile,
                    %error,
                    "failed to restore agent live session"
                );
                false
            }
        }
    }

    fn live_grid_size(&self, node_id: NodeId) -> (u16, u16) {
        let Some(NodeView::Term { snapshot, .. }) = self.node_view(node_id) else {
            return (DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS);
        };
        (usize_to_u16(snapshot.cols), usize_to_u16(snapshot.lines))
    }
}

fn checkpoint_grid_size(checkpoint: Option<&SessionCheckpoint>) -> (u16, u16) {
    checkpoint
        .map(|checkpoint| (checkpoint.cols, checkpoint.rows))
        .unwrap_or((DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS))
}

fn usize_to_u16(value: usize) -> u16 {
    value.min(usize::from(u16::MAX)) as u16
}

fn terminal_shell_command(shell: &str) -> PtyCommand {
    let command = PtyCommand::new(shell);
    if is_zsh_shell(shell) {
        command.arg("-l")
    } else {
        command
    }
}

fn is_zsh_shell(shell: &str) -> bool {
    Path::new(shell).file_name().and_then(|name| name.to_str()) == Some("zsh")
}

#[cfg(test)]
#[path = "runtime_storage_tests.rs"]
mod storage_tests;
#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "workspace_runtime_tests.rs"]
mod workspace_tests;
