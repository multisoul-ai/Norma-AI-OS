//! 节点生命周期 + LiveNode 注册表。把 canvas 文档与可运行节点组装在一起。

use crate::events::RuntimeEvents;
use crate::focus::{FocusState, InputMode};
use crate::live::{Caps, LiveNode, NodeError, NodeInput, NodeResize, NodeScroll, NodeView};
use crate::nodes::agent::spawn_agent_live;
use crate::nodes::term::TermLive;
use crate::runtime_support::terminal_shell_command;
use canvas::{
    AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, Document, DocumentSnapshot, NodeId,
    NodeType, PreviewSpec, PreviewType, Selection, TermSpec,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use storage::{
    CanvasStore, NodeSession, NodeSessionStore, SessionCheckpoint, SessionStore, StorageError,
    StorageResult,
};
#[cfg(test)]
use terminal::PtyCommand;
use terminal::{PtyError, TerminalSessionId};

pub const DEFAULT_TERMINAL_SHELL: &str = "/bin/zsh";
pub(crate) const DEFAULT_SESSION_COLS: u16 = 80;
pub(crate) const DEFAULT_SESSION_ROWS: u16 = 24;

pub struct Runtime {
    pub document: Document,
    pub selection: Selection,
    pub focus: FocusState,
    live: HashMap<NodeId, Box<dyn LiveNode>>,
    pub(crate) events: RuntimeEvents,
    pub(crate) agent_hook_config: Option<crate::agent_hooks::AgentHookRuntimeConfig>,
    terminal_shells: HashMap<NodeId, String>,
    pub(crate) agent_session_ids: HashMap<NodeId, String>,
    #[cfg(test)]
    test_agent_spawn_command: Option<PtyCommand>,
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
            events: RuntimeEvents::default(),
            agent_hook_config: None,
            terminal_shells: HashMap::new(),
            agent_session_ids: HashMap::new(),
            #[cfg(test)]
            test_agent_spawn_command: None,
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
        self.events.remove_all();
        self.document = Document::from_snapshot(snapshot);
        self.selection.clear();
        self.focus.exit_to_canvas();
        self.live.clear();
        self.terminal_shells.clear();
        self.agent_session_ids.clear();
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
        crate::session_restore::collect_session_checkpoints(
            &self.live,
            &self.document,
            &self.terminal_shells,
        )
    }

    pub fn save_session_checkpoints(&self, store: &SessionStore) -> StorageResult<()> {
        crate::session_restore::save_session_checkpoints(store, &self.collect_session_checkpoints())
    }

    pub fn restore_live_sessions(&mut self, checkpoints: &[SessionCheckpoint]) -> usize {
        let mut ctx = crate::session_restore::RestoreCtx {
            live: &mut self.live,
            terminal_shells: &mut self.terminal_shells,
            events: &mut self.events,
            next_session_id: &mut self.next_session_id,
            workspace_root: self.workspace_root.as_deref(),
            agent_hook_config: self.agent_hook_config.as_ref(),
        };
        crate::session_restore::restore_live_sessions(checkpoints, &self.document, &mut ctx)
    }

    pub fn save_node_sessions(&self, store: &NodeSessionStore) -> StorageResult<()> {
        crate::session_restore::save_node_sessions(
            store,
            &self.live,
            &self.document,
            self.workspace_root.as_deref(),
            &self.agent_session_ids,
        )
    }

    pub fn restore_live_sessions_with_node_sessions(
        &mut self,
        checkpoints: &[SessionCheckpoint],
        node_sessions: &[NodeSession],
    ) -> usize {
        let mut ctx = crate::session_restore::RestoreCtx {
            live: &mut self.live,
            terminal_shells: &mut self.terminal_shells,
            events: &mut self.events,
            next_session_id: &mut self.next_session_id,
            workspace_root: self.workspace_root.as_deref(),
            agent_hook_config: self.agent_hook_config.as_ref(),
        };
        crate::session_restore::restore_live_sessions_with_node_sessions(
            checkpoints,
            node_sessions,
            &self.document,
            &mut ctx,
        )
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
        let live = TermLive::spawn_command(sid, command, cols, rows).inspect_err(|error| {
            self.events
                .operation_failed(None, "spawn_terminal", error.to_string());
        })?;
        let node_id = self
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), bounds);
        self.terminal_shells.insert(node_id, shell.to_string());
        self.live.insert(node_id, Box::new(live));
        self.events.register(node_id, NodeType::Term);
        Ok(node_id)
    }

    pub fn spawn_agent(
        &mut self,
        profile: AgentProfile,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        self.spawn_agent_with_alias(profile, None, bounds, cols, rows)
    }

    pub fn spawn_agent_with_alias(
        &mut self,
        profile: AgentProfile,
        alias: Option<String>,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let node_id = self.add_agent_node(profile, alias, bounds);
        let sid = self.allocate_session_id();
        let live = self
            .spawn_agent_live_for_profile(node_id, sid, profile, cols, rows)
            .inspect_err(|error| {
                self.document.remove_node(node_id);
                self.events
                    .operation_failed(None, "spawn_agent", error.to_string());
            })?;
        self.register_agent_live_node(node_id, live);
        Ok(node_id)
    }

    pub fn spawn_preview_from_path(
        &mut self,
        path: PathBuf,
        explicit_type: Option<PreviewType>,
        bounds: CanvasRect,
    ) -> NodeId {
        self.document.add_node(
            CanvasNodeKind::Preview(PreviewSpec {
                path: path.to_string_lossy().into_owned(),
                explicit_type,
            }),
            bounds,
        )
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
        self.spawn_agent_with_command_and_alias(profile, None, bounds, command, cols, rows)
    }

    #[cfg(test)]
    fn spawn_agent_with_command_and_alias(
        &mut self,
        profile: AgentProfile,
        alias: Option<String>,
        bounds: CanvasRect,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.allocate_session_id();
        let live =
            TermLive::spawn_command(sid, command.with_cwd(self.workspace_root()), cols, rows)?;
        let node_id = self.add_agent_node(profile, alias, bounds);
        self.register_agent_live_node(node_id, live);
        Ok(node_id)
    }

    #[cfg(test)]
    fn agent_command(profile: AgentProfile) -> PtyCommand {
        crate::nodes::agent::agent_command(profile, None)
    }

    #[cfg(test)]
    fn set_test_agent_spawn_command(&mut self, command: PtyCommand) {
        self.test_agent_spawn_command = Some(command);
    }

    fn add_agent_node(
        &mut self,
        profile: AgentProfile,
        alias: Option<String>,
        bounds: CanvasRect,
    ) -> NodeId {
        self.document
            .add_node(CanvasNodeKind::Agent(AgentSpec { profile, alias }), bounds)
    }

    fn register_agent_live_node(&mut self, node_id: NodeId, live: TermLive) {
        self.live.insert(node_id, Box::new(live));
        self.events.register(node_id, NodeType::Agent);
    }

    fn spawn_agent_live_for_profile(
        &self,
        node_id: NodeId,
        sid: TerminalSessionId,
        profile: AgentProfile,
        cols: u16,
        rows: u16,
    ) -> Result<TermLive, PtyError> {
        #[cfg(test)]
        if let Some(command) = &self.test_agent_spawn_command {
            return TermLive::spawn_command(
                sid,
                command.with_cwd(self.workspace_root()),
                cols,
                rows,
            );
        }

        let hook_env = self.agent_hook_env(node_id, profile);
        spawn_agent_live(
            sid,
            profile,
            self.workspace_root(),
            hook_env.as_ref(),
            cols,
            rows,
        )
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
            return self.fail_node_operation(Some(node_id), "resize", NodeError::MissingNode);
        };
        let result = node.resize(resize);
        self.finish_node_operation(Some(node_id), "resize", result)
    }

    pub fn send_node_input(&mut self, node_id: NodeId, input: NodeInput) -> Result<(), NodeError> {
        let Some(node) = self.live.get_mut(&node_id) else {
            return self.fail_node_operation(Some(node_id), "input", NodeError::MissingNode);
        };
        let result = node.input(input);
        self.finish_node_operation(Some(node_id), "input", result)
    }

    pub fn scroll_node(&mut self, node_id: NodeId, scroll: NodeScroll) -> Result<bool, NodeError> {
        let Some(node) = self.live.get_mut(&node_id) else {
            return self.fail_node_operation(Some(node_id), "scroll", NodeError::MissingNode);
        };
        let result = node.scroll(scroll);
        self.finish_node_operation(Some(node_id), "scroll", result)
    }

    /// drain 所有 live node；任一 dirty 则返回 true（app 据此决定是否 repaint）。
    pub fn drain_all(&mut self) -> bool {
        let mut dirty = false;
        let mut statuses = Vec::new();
        for (node_id, node) in self.live.iter_mut() {
            if node.drain() {
                dirty = true;
            }
            if let Some(status) = node.terminal_status() {
                statuses.push((*node_id, status));
            }
        }
        for (node_id, status) in statuses {
            self.events.update_from_terminal_status(node_id, status);
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
        self.events.remove(node_id);
        self.terminal_shells.remove(&node_id);
        self.agent_session_ids.remove(&node_id);

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

    pub fn agent_session_id(&self, node_id: NodeId) -> Option<String> {
        self.agent_session_ids.get(&node_id).cloned()
    }

    fn finish_node_operation<T>(
        &mut self,
        node_id: Option<NodeId>,
        operation: &str,
        result: Result<T, NodeError>,
    ) -> Result<T, NodeError> {
        if let Err(error) = &result {
            self.events
                .operation_failed(node_id, operation, error.to_string());
        }
        result
    }

    fn fail_node_operation<T>(
        &mut self,
        node_id: Option<NodeId>,
        operation: &str,
        error: NodeError,
    ) -> Result<T, NodeError> {
        self.events
            .operation_failed(node_id, operation, error.to_string());
        Err(error)
    }
}

#[cfg(test)]
#[path = "../tests/unit/agent_alias_tests.rs"]
mod agent_alias_tests;
#[cfg(test)]
#[path = "../tests/unit/agent_command_tests.rs"]
mod agent_command_tests;
#[cfg(test)]
#[path = "../tests/unit/agent_session_snapshot_tests.rs"]
mod agent_session_snapshot_tests;
#[cfg(test)]
#[path = "../tests/unit/agent_task_event_tests.rs"]
mod agent_task_event_tests;
#[cfg(test)]
#[path = "../tests/unit/runtime_event_tests.rs"]
mod event_tests;
#[cfg(test)]
#[path = "../tests/unit/preview_tests.rs"]
mod preview_tests;
#[cfg(test)]
#[path = "../tests/unit/runtime_scroll_tests.rs"]
mod scroll_tests;
#[cfg(test)]
#[path = "../tests/unit/runtime_storage_tests.rs"]
mod storage_tests;
#[cfg(test)]
#[path = "../tests/unit/runtime_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "../tests/unit/workspace_runtime_tests.rs"]
mod workspace_tests;
