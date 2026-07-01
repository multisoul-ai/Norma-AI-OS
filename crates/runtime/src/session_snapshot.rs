//! Session snapshot: capture cwd + Claude session IDs at exit,
//! and build PtyCommands that restore them at next open.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use canvas::node::{AgentProfile, CanvasNodeKind};
use canvas::{Document, NodeId};
use serde_json::Value;
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
                    AgentProfile::Codex => find_codex_session_id(root),
                    AgentProfile::OpenCode => None,
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
/// Path encoding: replace every '/' with '-'.
/// Example: /Users/alan/project → -Users-alan-project
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
/// replace every '/', including the leading absolute-path slash, with '-'.
fn encode_claude_path(path: &str) -> String {
    path.replace('/', "-")
}

/// Find the most recently modified Codex interactive session for a workspace.
pub fn find_codex_session_id(workspace_root: &Path) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    find_codex_session_id_inner(workspace_root, &Path::new(&home).join(".codex"))
}

/// Testable inner function that accepts an explicit CODEX_HOME directory.
pub fn find_codex_session_id_inner(workspace_root: &Path, codex_home: &Path) -> Option<String> {
    let sessions_dir = codex_home.join("sessions");
    let canonical_workspace = workspace_root.canonicalize().ok()?;
    let mut best: Option<(std::time::SystemTime, String)> = None;
    visit_codex_session_files(&sessions_dir, &mut |path, modified| {
        let Some(session_id) = codex_session_id_for_workspace(path, &canonical_workspace) else {
            return;
        };
        if best
            .as_ref()
            .map(|(best_modified, _)| modified > *best_modified)
            .unwrap_or(true)
        {
            best = Some((modified, session_id));
        }
    });
    best.map(|(_, session_id)| session_id)
}

/// Build a PtyCommand for a terminal node that opens in the saved cwd.
pub fn build_terminal_restore_command(shell: &str, cwd: &str) -> PtyCommand {
    PtyCommand::new(shell).args(["-l"]).cwd(PathBuf::from(cwd))
}

/// Build a PtyCommand for an agent that resumes a saved CLI session.
pub(crate) fn build_agent_resume_command(
    profile: AgentProfile,
    shell: &str,
    session_id: &str,
    hook_env: Option<&AgentHookEnv>,
    workspace_root: Option<&Path>,
) -> PtyCommand {
    let cmd_str = match profile {
        AgentProfile::Claude => format!(
            "exec claude --dangerously-skip-permissions --resume {}",
            shell_quote(session_id)
        ),
        AgentProfile::Codex => {
            let mut command = "exec codex --dangerously-bypass-approvals-and-sandbox".to_string();
            if hook_env.is_some() {
                command.push_str(" --dangerously-bypass-hook-trust");
            }
            if let Some(root) = workspace_root {
                command.push_str(" --cd ");
                command.push_str(&shell_quote(&root.to_string_lossy()));
            }
            command.push_str(" resume ");
            command.push_str(&shell_quote(session_id));
            command
        }
        AgentProfile::OpenCode => {
            format!(
                "exec \"${{OPENCODE_BIN:-opencode}}\" resume {}",
                shell_quote(session_id)
            )
        }
    };
    let mut cmd = PtyCommand::new(shell).args(["-ic", &cmd_str]);
    cmd = cmd.with_cwd(workspace_root);
    match hook_env {
        Some(env) => env.apply_to_command(cmd),
        None => cmd,
    }
}

fn visit_codex_session_files(dir: &Path, visit: &mut dyn FnMut(&Path, std::time::SystemTime)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            visit_codex_session_files(&path, visit);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        visit(&path, modified);
    }
}

fn codex_session_id_for_workspace(path: &Path, workspace_root: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    for line in contents.lines().take(8) {
        let value: Value = serde_json::from_str(line).ok()?;
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }
        let payload = value.get("payload")?;
        let cwd = payload.get("cwd").and_then(Value::as_str)?;
        let cwd = Path::new(cwd).canonicalize().ok()?;
        if cwd != workspace_root {
            return None;
        }
        return payload
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string);
    }
    None
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
#[path = "../tests/unit/session_snapshot_tests.rs"]
mod tests;
