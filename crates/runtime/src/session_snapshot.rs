//! Session snapshot: capture cwd + Claude session IDs at exit,
//! and build PtyCommands that restore them at next open.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use canvas::node::{AgentProfile, CanvasNodeKind};
use canvas::{Document, NodeId};
use storage::{NodeSession, NodeSessionKind};
use terminal::pty::PtyCommand;

use crate::agent_hooks::AgentHookEnv;
use crate::live::LiveNode;
use crate::macos_proc;

/// Collect cwd and session_id for every live node at app exit.
/// Called before kill_all() so PTY processes are still alive.
pub fn collect_node_sessions(
    live: &HashMap<NodeId, Box<dyn LiveNode>>,
    document: &Document,
    workspace_root: Option<&Path>,
) -> Vec<NodeSession> {
    let mut sessions = Vec::new();

    for node in document.nodes() {
        let Some(live_node) = live.get(&node.id) else {
            continue;
        };

        match &node.kind {
            CanvasNodeKind::Term(_) => {
                let cwd = live_node.child_pid().and_then(macos_proc::process_cwd);
                sessions.push(NodeSession {
                    node_id: node.id,
                    kind: NodeSessionKind::Terminal,
                    cwd,
                    session_id: None,
                });
            }
            CanvasNodeKind::Agent(spec) => {
                let session_id = workspace_root.and_then(|root| match spec.profile {
                    AgentProfile::Claude => find_agent_session_id(root),
                    AgentProfile::Codex => {
                        // TODO(Phase 1b): implement Codex session file discovery
                        None
                    }
                });
                sessions.push(NodeSession {
                    node_id: node.id,
                    kind: NodeSessionKind::Agent,
                    cwd: None,
                    session_id,
                });
            }
            _ => {}
        }
    }

    sessions
}

/// Find the most recently modified Claude session file for the given workspace.
///
/// Claude stores sessions at: ~/.claude/projects/<encoded-path>/<uuid>.jsonl
/// Path encoding: strip leading '/', replace '/' with '-'.
/// Example: /Users/alan/project → Users-alan-project
pub fn find_agent_session_id(workspace_root: &Path) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    find_agent_session_id_inner(workspace_root, Path::new(&home))
}

/// Testable inner function that accepts an explicit home directory.
pub fn find_agent_session_id_inner(workspace_root: &Path, home: &Path) -> Option<String> {
    let encoded = encode_claude_path(workspace_root.to_str()?);
    let projects_dir = home.join(".claude").join("projects").join(encoded);

    let entries = std::fs::read_dir(&projects_dir).ok()?;

    let mut best: Option<(std::time::SystemTime, String)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(mtime) = meta.modified() else {
            continue;
        };

        if best.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
            best = Some((mtime, stem.to_string()));
        }
    }

    best.map(|(_, stem)| stem)
}

/// Encode a workspace path the same way Claude Code does:
/// strip leading '/', replace '/' with '-'.
fn encode_claude_path(path: &str) -> String {
    path.trim_start_matches('/').replace('/', "-")
}

/// Build a PtyCommand for a terminal node that opens in the saved cwd.
pub fn build_terminal_restore_command(shell: &str, cwd: &str) -> PtyCommand {
    PtyCommand::new(shell).args(["-l"]).cwd(PathBuf::from(cwd))
}

/// Build a PtyCommand for a Claude agent that resumes the saved session.
pub(crate) fn build_resume_command(
    shell: &str,
    session_id: &str,
    hook_env: Option<&AgentHookEnv>,
) -> PtyCommand {
    let cmd_str = format!("exec claude --dangerously-skip-permissions --resume {session_id}");
    let cmd = PtyCommand::new(shell).args(["-ic", &cmd_str]);
    match hook_env {
        Some(env) => env.apply_to_command(cmd),
        None => cmd,
    }
}

#[cfg(test)]
#[path = "../tests/unit/session_snapshot_tests.rs"]
mod tests;
