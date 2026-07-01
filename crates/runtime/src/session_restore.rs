//! Session checkpoint and node-session save/restore helpers.
//! Extracted from runtime.rs to keep that file under 500 lines.

use std::collections::HashMap;
use std::path::Path;

use canvas::{AgentProfile, CanvasNodeKind, Document, NodeId, NodeType};
use storage::{
    NodeSession, NodeSessionKind, NodeSessionStore, SessionCheckpoint, SessionCheckpointKind,
    SessionStore, StorageResult,
};
use terminal::pty::PtyCommand;

use crate::agent_hooks::{AgentHookEnv, AgentHookRuntimeConfig};
use crate::events::RuntimeEvents;
use crate::live::LiveNode;
use crate::nodes::agent::{agent_command, spawn_agent_live};
use crate::nodes::term::TermLive;
use crate::runtime::{DEFAULT_SESSION_COLS, DEFAULT_SESSION_ROWS, DEFAULT_TERMINAL_SHELL};
use crate::runtime_support::{checkpoint_grid_size, terminal_shell_command, usize_to_u16};
use crate::{session_snapshot, NodeView};

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
                CanvasNodeKind::Rect(_) | CanvasNodeKind::Preview(_) => return None,
            };
            let (cols, rows) = live_grid_size(live, node.id);
            Some(SessionCheckpoint {
                node_id: node.id,
                kind,
                payload: node.kind.display_title(),
                cols,
                rows,
                shell: match &node.kind {
                    CanvasNodeKind::Term(_) => Some(
                        terminal_shells
                            .get(&node.id)
                            .cloned()
                            .unwrap_or_else(|| DEFAULT_TERMINAL_SHELL.to_string()),
                    ),
                    CanvasNodeKind::Agent(_)
                    | CanvasNodeKind::Rect(_)
                    | CanvasNodeKind::Preview(_) => None,
                },
                agent_profile: match &node.kind {
                    CanvasNodeKind::Agent(spec) => Some(spec.profile),
                    CanvasNodeKind::Term(_)
                    | CanvasNodeKind::Rect(_)
                    | CanvasNodeKind::Preview(_) => None,
                },
            })
        })
        .collect()
}

/// Persist all checkpoints to the store (INSERT OR REPLACE).
pub fn save_session_checkpoints(
    store: &SessionStore,
    checkpoints: &[SessionCheckpoint],
) -> StorageResult<()> {
    for checkpoint in checkpoints {
        store.save_checkpoint(checkpoint)?;
    }
    Ok(())
}

/// Context needed by restore functions to attach live nodes.
pub struct RestoreCtx<'a> {
    pub live: &'a mut HashMap<NodeId, Box<dyn LiveNode>>,
    pub terminal_shells: &'a mut HashMap<NodeId, String>,
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
                let profile = agent_checkpoint
                    .and_then(|cp| cp.agent_profile)
                    .unwrap_or(spec.profile);
                attach_agent_live(node_id, profile, cols, rows, ctx)
            }
            CanvasNodeKind::Rect(_) | CanvasNodeKind::Preview(_) => false,
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
                let profile = agent_checkpoint
                    .and_then(|cp| cp.agent_profile)
                    .unwrap_or(spec.profile);
                let hook_env = ctx.make_hook_env(node_id, profile);

                // Use profile-specific resume command if a saved or discoverable session id
                // is available.
                let use_resume =
                    resolve_agent_restore_session_id(node_session, profile, ctx.workspace_root);

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
            CanvasNodeKind::Rect(_) | CanvasNodeKind::Preview(_) => false,
        };
        if did_restore {
            restored += 1;
        }
    }
    restored
}

/// Capture cwd + session_id for all live nodes and persist to the store.
/// Must be called before kill_all() so PTY processes are still alive.
pub fn save_node_sessions(
    store: &NodeSessionStore,
    live: &HashMap<NodeId, Box<dyn LiveNode>>,
    document: &Document,
    workspace_root: Option<&Path>,
) -> StorageResult<()> {
    let sessions = session_snapshot::collect_node_sessions(live, document, workspace_root);
    for session in &sessions {
        store.save_session(session)?;
    }
    Ok(())
}

fn resolve_agent_restore_session_id(
    node_session: Option<&NodeSession>,
    profile: AgentProfile,
    workspace_root: Option<&Path>,
) -> Option<String> {
    resolve_agent_restore_session_id_with_discovery(
        node_session,
        profile,
        workspace_root,
        discover_agent_session_id,
    )
}

fn resolve_agent_restore_session_id_with_discovery<F>(
    node_session: Option<&NodeSession>,
    profile: AgentProfile,
    workspace_root: Option<&Path>,
    discover: F,
) -> Option<String>
where
    F: FnOnce(AgentProfile, &Path) -> Option<String>,
{
    if !matches!(profile, AgentProfile::Claude | AgentProfile::Codex) {
        return None;
    }
    if let Some(saved) = node_session
        .filter(|s| s.kind == NodeSessionKind::Agent)
        .and_then(|s| s.session_id.as_deref())
        .filter(|session_id| !session_id.is_empty())
    {
        return Some(saved.to_string());
    }
    workspace_root.and_then(|root| discover(profile, root))
}

fn discover_agent_session_id(profile: AgentProfile, workspace_root: &Path) -> Option<String> {
    match profile {
        AgentProfile::Claude => session_snapshot::find_agent_session_id(workspace_root),
        AgentProfile::Codex => session_snapshot::find_codex_session_id(workspace_root),
        AgentProfile::OpenCode => None,
    }
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

// ── Test helpers ───────────────────────────────────────────────────────────

/// Test helper: return the PtyCommand that would be used for agent restore.
#[cfg(test)]
pub fn build_agent_restore_command_for_test(
    session: &NodeSession,
    profile: AgentProfile,
    hook_env: Option<&AgentHookEnv>,
    workspace_root: Option<&Path>,
) -> PtyCommand {
    let session_id = resolve_agent_restore_session_id_with_discovery(
        Some(session),
        profile,
        workspace_root,
        |_, _| None,
    );
    match session_id.as_deref() {
        Some(id) => session_snapshot::build_agent_resume_command(
            profile,
            DEFAULT_TERMINAL_SHELL,
            id,
            hook_env,
            workspace_root,
        ),
        None => agent_command(profile, hook_env),
    }
}

#[cfg(test)]
pub fn resolve_agent_restore_session_id_for_test(
    session: Option<&NodeSession>,
    profile: AgentProfile,
    workspace_root: Option<&Path>,
    discovered_session_id: Option<&str>,
) -> Option<String> {
    resolve_agent_restore_session_id_with_discovery(session, profile, workspace_root, |_, _| {
        discovered_session_id.map(ToString::to_string)
    })
}

/// Test helper: return the PtyCommand that would be used for terminal restore.
#[cfg(test)]
pub fn build_terminal_restore_command_for_test(session: &NodeSession, shell: &str) -> PtyCommand {
    match &session.cwd {
        Some(cwd) => session_snapshot::build_terminal_restore_command(shell, cwd),
        None => terminal_shell_command(shell),
    }
}
