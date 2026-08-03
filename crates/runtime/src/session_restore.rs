//! Session checkpoint and node-session save/restore helpers.
//! Extracted from runtime.rs to keep that file under 500 lines.

use std::collections::HashMap;
use std::path::Path;

use canvas::{AgentProfile, CanvasNodeKind, Document, NodeId, NodeType};
use storage::NodeSessionKind;
use storage::{
    NodeSession, NodeSessionStore, SessionCheckpoint, SessionCheckpointKind, SessionStore,
    StorageResult,
};
use terminal::pty::PtyCommand;

use crate::agent_hooks::{AgentHookEnv, AgentHookRuntimeConfig};
use crate::agent_session_bindings::AgentSessionBindings;
use crate::events::RuntimeEvents;
use crate::live::LiveNode;
use crate::nodes::agent::{agent_command, spawn_agent_live};
use crate::nodes::term::TermLive;
use crate::runtime::{DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS, DEFAULT_TERMINAL_SHELL};
use crate::runtime_support::{checkpoint_grid_size, terminal_shell_command, usize_to_u16};
use crate::{session_snapshot, NodeView};

#[cfg(test)]
#[path = "session_restore_test_support.rs"]
mod test_support;
#[cfg(test)]
pub use test_support::{
    build_agent_restore_command_for_test, build_agent_restore_command_with_discovery_for_test,
    build_terminal_restore_command_for_test,
};

// ── Moved from runtime.rs ──────────────────────────────────────────────────

/// Collect session checkpoints for all live nodes.
pub fn collect_session_checkpoints(
    live: &HashMap<NodeId, Box<dyn LiveNode>>,
    document: &Document,
    terminal_shells: &HashMap<NodeId, String>,
) -> Vec<SessionCheckpoint> {
    document
        .nodes()
        .iter()
        .filter(|node| live.contains_key(&node.id))
        .filter_map(|node| {
            let kind = match &node.kind {
                CanvasNodeKind::Term(_) => SessionCheckpointKind::Terminal,
                CanvasNodeKind::Agent(_) => SessionCheckpointKind::Agent,
                CanvasNodeKind::Rect(_)
                | CanvasNodeKind::Preview(_)
                | CanvasNodeKind::Browser(_) => return None,
            };
            let (cols, rows) = live_grid_size(live, node.id);
            Some(SessionCheckpoint {
                node_id: node.id,
                kind,
                payload: node.kind.display_title(),
                cols,
                rows,
                shell: if matches!(&node.kind, CanvasNodeKind::Term(_)) {
                    Some(
                        terminal_shells
                            .get(&node.id)
                            .cloned()
                            .unwrap_or_else(|| DEFAULT_TERMINAL_SHELL.to_string()),
                    )
                } else {
                    None
                },
                agent_profile: agent_profile_for_checkpoint(&node.kind),
            })
        })
        .collect()
}

fn agent_profile_for_checkpoint(kind: &CanvasNodeKind) -> Option<AgentProfile> {
    match kind {
        CanvasNodeKind::Agent(spec) => Some(spec.profile),
        _ => None,
    }
}

/// Canvas document 是节点当前 profile 的权威来源；checkpoint 只恢复 grid 元数据。
pub(crate) fn resolve_agent_restore_profile(
    document_profile: AgentProfile,
    _checkpoint: Option<&SessionCheckpoint>,
) -> AgentProfile {
    document_profile
}

/// Persist all checkpoints to the store (INSERT OR REPLACE).
pub fn save_session_checkpoints(
    store: &SessionStore,
    checkpoints: &[SessionCheckpoint],
) -> StorageResult<()> {
    store.replace_all_checkpoints(checkpoints)
}

/// Context needed by restore functions to attach live nodes.
pub struct RestoreCtx<'a> {
    pub live: &'a mut HashMap<NodeId, Box<dyn LiveNode>>,
    pub terminal_shells: &'a mut HashMap<NodeId, String>,
    pub agent_session_bindings: &'a AgentSessionBindings,
    pub events: &'a mut RuntimeEvents,
    pub next_session_id: &'a mut u64,
    pub workspace_root: Option<&'a Path>,
    pub agent_hook_config: Option<&'a AgentHookRuntimeConfig>,
}

impl RestoreCtx<'_> {
    fn make_hook_env(&self, node_id: NodeId, profile: AgentProfile) -> Option<AgentHookEnv> {
        self.agent_hook_config.map(|config| AgentHookEnv {
            node_id,
            profile,
            endpoint: config.endpoint.clone(),
            token: config.token.clone(),
            workspace_id: config.workspace_id.clone(),
            workspace_route_id: config.workspace_route_id.clone(),
        })
    }
}

/// Restore live sessions from checkpoints.
pub fn restore_live_sessions(
    checkpoints: &[SessionCheckpoint],
    document: &Document,
    ctx: &mut RestoreCtx<'_>,
) -> usize {
    let checkpoints_by_node_id: HashMap<NodeId, &SessionCheckpoint> =
        checkpoints.iter().map(|cp| (cp.node_id, cp)).collect();

    let nodes: Vec<(NodeId, CanvasNodeKind)> = document
        .nodes()
        .iter()
        .map(|node| (node.id, node.kind.clone()))
        .collect();

    let mut restored = 0;
    for (node_id, kind) in nodes {
        if ctx.live.contains_key(&node_id) {
            continue;
        }
        let checkpoint = checkpoints_by_node_id.get(&node_id).copied();
        let did_restore = match kind {
            CanvasNodeKind::Term(_) => {
                let terminal_checkpoint =
                    checkpoint.filter(|cp| cp.kind == SessionCheckpointKind::Terminal);
                let (cols, rows) = checkpoint_grid_size(terminal_checkpoint);
                let shell = terminal_checkpoint
                    .and_then(|cp| cp.shell.as_deref())
                    .unwrap_or(DEFAULT_TERMINAL_SHELL);
                attach_terminal_live(node_id, shell, cols, rows, ctx)
            }
            CanvasNodeKind::Agent(spec) => {
                let agent_checkpoint =
                    checkpoint.filter(|cp| cp.kind == SessionCheckpointKind::Agent);
                let (cols, rows) = checkpoint_grid_size(agent_checkpoint);
                let profile = resolve_agent_restore_profile(spec.profile, agent_checkpoint);
                attach_agent_live(node_id, profile, cols, rows, ctx)
            }
            _ => false,
        };
        if did_restore {
            restored += 1;
        }
    }
    restored
}

/// Like `restore_live_sessions`, but applies cwd and `--resume` from node_sessions.
pub fn restore_live_sessions_with_node_sessions(
    checkpoints: &[SessionCheckpoint],
    node_sessions: &[NodeSession],
    document: &Document,
    ctx: &mut RestoreCtx<'_>,
) -> usize {
    let checkpoints_by_node_id: HashMap<NodeId, &SessionCheckpoint> =
        checkpoints.iter().map(|cp| (cp.node_id, cp)).collect();
    let sessions_by_node_id: HashMap<NodeId, &NodeSession> =
        node_sessions.iter().map(|s| (s.node_id, s)).collect();

    let nodes: Vec<(NodeId, CanvasNodeKind)> = document
        .nodes()
        .iter()
        .map(|node| (node.id, node.kind.clone()))
        .collect();

    let mut restored = 0;
    for (node_id, kind) in nodes {
        if ctx.live.contains_key(&node_id) {
            continue;
        }
        let checkpoint = checkpoints_by_node_id.get(&node_id).copied();
        let node_session = sessions_by_node_id.get(&node_id).copied();

        let did_restore = match kind {
            CanvasNodeKind::Term(_) => {
                let terminal_checkpoint =
                    checkpoint.filter(|cp| cp.kind == SessionCheckpointKind::Terminal);
                let (cols, rows) = checkpoint_grid_size(terminal_checkpoint);
                let shell = terminal_checkpoint
                    .and_then(|cp| cp.shell.as_deref())
                    .unwrap_or(DEFAULT_TERMINAL_SHELL);

                // Apply saved cwd if available.
                let command = match node_session.and_then(|s| s.cwd.as_deref()) {
                    Some(cwd) => session_snapshot::build_terminal_restore_command(shell, cwd),
                    None => terminal_shell_command(shell).with_cwd(ctx.workspace_root),
                };
                attach_terminal_live_with_command(node_id, shell, command, cols, rows, ctx)
            }
            CanvasNodeKind::Agent(spec) => {
                let agent_checkpoint =
                    checkpoint.filter(|cp| cp.kind == SessionCheckpointKind::Agent);
                let (cols, rows) = checkpoint_grid_size(agent_checkpoint);
                let profile = resolve_agent_restore_profile(spec.profile, agent_checkpoint);
                let hook_env = ctx.make_hook_env(node_id, profile);

                // Resume only from the session id persisted for this node. Workspace opening
                // must never scan provider history directories on the foreground thread.
                let use_resume = ctx
                    .agent_session_bindings
                    .session_id(node_id)
                    .map(ToString::to_string);

                let command = match use_resume.as_deref() {
                    Some(session_id) => session_snapshot::build_agent_resume_command(
                        profile,
                        DEFAULT_TERMINAL_SHELL,
                        session_id,
                        hook_env.as_ref(),
                        ctx.workspace_root,
                    ),
                    None => agent_command(profile, hook_env.as_ref()).with_cwd(ctx.workspace_root),
                };
                attach_agent_live_with_command(node_id, command, cols, rows, ctx)
            }
            _ => false,
        };
        if did_restore {
            restored += 1;
        }
    }
    restored
}

pub fn restore_live_session_for_node_with_node_sessions(
    node_id: NodeId,
    checkpoints: &[SessionCheckpoint],
    node_sessions: &[NodeSession],
    _discovered_session_id: Option<&str>,
    document: &Document,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    if ctx.live.contains_key(&node_id) {
        return false;
    }
    let Some(node) = document.node(node_id) else {
        return false;
    };
    let checkpoint = checkpoints.iter().find(|cp| cp.node_id == node_id);
    let node_session = node_sessions
        .iter()
        .find(|session| session.node_id == node_id);

    match node.kind.clone() {
        CanvasNodeKind::Term(_) => {
            let terminal_checkpoint =
                checkpoint.filter(|cp| cp.kind == SessionCheckpointKind::Terminal);
            let (cols, rows) = checkpoint_grid_size(terminal_checkpoint);
            let shell = terminal_checkpoint
                .and_then(|cp| cp.shell.as_deref())
                .unwrap_or(DEFAULT_TERMINAL_SHELL);
            let command = match node_session.and_then(|s| s.cwd.as_deref()) {
                Some(cwd) => session_snapshot::build_terminal_restore_command(shell, cwd),
                None => terminal_shell_command(shell).with_cwd(ctx.workspace_root),
            };
            attach_terminal_live_with_command(node_id, shell, command, cols, rows, ctx)
        }
        CanvasNodeKind::Agent(spec) => {
            let agent_checkpoint = checkpoint.filter(|cp| cp.kind == SessionCheckpointKind::Agent);
            let (cols, rows) = checkpoint_grid_size(agent_checkpoint);
            let profile = resolve_agent_restore_profile(spec.profile, agent_checkpoint);
            let hook_env = ctx.make_hook_env(node_id, profile);
            let use_resume = ctx
                .agent_session_bindings
                .session_id(node_id)
                .map(ToString::to_string);
            let command = match use_resume.as_deref() {
                Some(session_id) => session_snapshot::build_agent_resume_command(
                    profile,
                    DEFAULT_TERMINAL_SHELL,
                    session_id,
                    hook_env.as_ref(),
                    ctx.workspace_root,
                ),
                None => agent_command(profile, hook_env.as_ref()).with_cwd(ctx.workspace_root),
            };
            attach_agent_live_with_command(node_id, command, cols, rows, ctx)
        }
        CanvasNodeKind::Rect(_) | CanvasNodeKind::Preview(_) | CanvasNodeKind::Browser(_) => false,
    }
}

/// Capture cwd + session_id for all live nodes and persist to the store.
/// Must be called before kill_all() so PTY processes are still alive.
pub fn save_node_sessions(
    store: &NodeSessionStore,
    live: &HashMap<NodeId, Box<dyn LiveNode>>,
    document: &Document,
    workspace_root: Option<&Path>,
    agent_session_bindings: &AgentSessionBindings,
) -> StorageResult<()> {
    let sessions = session_snapshot::collect_node_sessions(
        live,
        document,
        workspace_root,
        agent_session_bindings,
    );
    let current_nodes = document
        .nodes()
        .iter()
        .filter_map(|node| match &node.kind {
            CanvasNodeKind::Term(_) => Some((node.id, NodeSessionKind::Terminal)),
            CanvasNodeKind::Agent(_) => Some((node.id, NodeSessionKind::Agent)),
            CanvasNodeKind::Rect(_) | CanvasNodeKind::Preview(_) | CanvasNodeKind::Browser(_) => {
                None
            }
        })
        .collect::<Vec<_>>();
    store.reconcile_sessions(&current_nodes, &sessions)
}

// ── Internal attach helpers ────────────────────────────────────────────────

fn allocate_session_id(next: &mut u64) -> u64 {
    let sid = *next;
    *next += 1;
    sid
}

fn live_grid_size(live: &HashMap<NodeId, Box<dyn LiveNode>>, node_id: NodeId) -> (u16, u16) {
    let Some(NodeView::Term { snapshot, .. }) = live.get(&node_id).and_then(|n| n.view()) else {
        return (DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS);
    };
    (usize_to_u16(snapshot.cols), usize_to_u16(snapshot.lines))
}

fn attach_terminal_live(
    node_id: NodeId,
    shell: &str,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let command = terminal_shell_command(shell).with_cwd(ctx.workspace_root);
    attach_terminal_live_with_command(node_id, shell, command, cols, rows, ctx)
}

fn attach_terminal_live_with_command(
    node_id: NodeId,
    shell: &str,
    command: PtyCommand,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let sid = allocate_session_id(ctx.next_session_id);
    match TermLive::spawn_command(sid, command, cols, rows) {
        Ok(live) => {
            ctx.terminal_shells.insert(node_id, shell.to_string());
            ctx.live.insert(node_id, Box::new(live));
            ctx.events.register(node_id, NodeType::Term);
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
    node_id: NodeId,
    profile: AgentProfile,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let hook_env = ctx.make_hook_env(node_id, profile);
    let sid = allocate_session_id(ctx.next_session_id);
    match spawn_agent_live(
        sid,
        profile,
        ctx.workspace_root,
        hook_env.as_ref(),
        cols,
        rows,
        None,
    ) {
        Ok(live) => {
            ctx.live.insert(node_id, Box::new(live));
            ctx.events.register(node_id, NodeType::Agent);
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

fn attach_agent_live_with_command(
    node_id: NodeId,
    command: PtyCommand,
    cols: u16,
    rows: u16,
    ctx: &mut RestoreCtx<'_>,
) -> bool {
    let sid = allocate_session_id(ctx.next_session_id);
    match TermLive::spawn_command(sid, command, cols, rows) {
        Ok(live) => {
            ctx.live.insert(node_id, Box::new(live));
            ctx.events.register(node_id, NodeType::Agent);
            true
        }
        Err(error) => {
            tracing::warn!(
                ?node_id,
                %error,
                "failed to restore agent live session with command"
            );
            false
        }
    }
}
