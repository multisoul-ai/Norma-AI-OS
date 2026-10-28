//! Cursor keeps the original TUI until its foreground session AND call can be proven.
//! A launch-time session or matching screen text cannot disambiguate switched/child sessions.
use super::*;
use crate::runtime::LaunchControl;
use std::sync::Arc;
use terminal::PtyEvent;

const EXEC: &str = "exec cursor-agent";
fn safe_session(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub(super) fn owned_identity(command: &PtyCommand) -> Option<(u64, String, Option<String>)> {
    let env = |key: &str| {
        command
            .env
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    if env("MULTISOUL_AGENT_PROFILE")? != "cursor"
        || command.args.len() != 2
        || command.args[0] != "-lic"
    {
        return None;
    }
    let resume = if command.args[1] == EXEC {
        None
    } else {
        let value = command.args[1]
            .strip_prefix(&format!("{EXEC} --resume '"))?
            .strip_suffix('\'')?;
        if !safe_session(value) {
            return None;
        }
        Some(value.into())
    };
    let process = env(norma::agent_hook::ENV_CONTROL_RUN_ID)?;
    if process.is_empty() || process.len() > 128 {
        return None;
    }
    Some((
        env("MULTISOUL_NODE_ID")?.parse().ok()?,
        process.into(),
        resume,
    ))
}

pub(super) fn prepare(
    id: TerminalSessionId,
    mut command: PtyCommand,
    cols: u16,
    rows: u16,
    control: Arc<LaunchControl>,
) -> Result<PreparedAgentTerminal, PtyError> {
    let (_, _, resume) =
        owned_identity(&command).ok_or_else(|| PtyError::Spawn("cursor owner missing".into()))?;
    command.args[1] = format!(
        "exec \"${{CURSOR_AGENT_BIN:-cursor-agent}}\"{}",
        resume
            .map(|id| format!(" --resume {}", crate::session_snapshot::shell_quote(&id)))
            .unwrap_or_default()
    );
    let backend = deferred::DeferredPty::new(control.clone(), cols, rows);
    let slot = backend.preparation();
    let (sender, receiver) = crossbeam_channel::unbounded();
    std::thread::Builder::new()
        .name("cursor-native-launch".into())
        .spawn(move || {
            let (cols, rows) = slot.dimensions();
            match control.spawn_pty(command, cols, rows, sender.clone()) {
                Ok(native) => {
                    if slot.install(native).is_ok() {
                        slot.maintain_sizes();
                    } else {
                        control.cancel();
                    }
                }
                _ => {
                    control.cancel();
                    let _ = sender.send(PtyEvent::Exited { code: Some(1) });
                }
            }
        })?;
    Ok(PreparedAgentTerminal {
        id,
        cols,
        rows,
        backend: Box::new(backend),
        receiver,
        // Store readers and the terminal engine remain independently testable; no production
        // launch supplies a host until Cursor exposes authoritative foreground identity.
        host: None,
    })
}
