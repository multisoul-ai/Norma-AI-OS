//! Background ownership of a private app-server, transparent relay and original native TUI.
use super::*;
use crate::runtime::LaunchControl;
use norma::codex::{native_questions::CodexQuestionHost, relay::CodexQuestionRelay};
use std::os::unix::net::UnixListener;
use std::sync::Arc;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;
use terminal::PtyEvent;

pub(super) fn owned_identity(command: &PtyCommand) -> Option<(u64, String)> {
    let env = |key: &str| {
        command
            .env
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    if env("MULTISOUL_AGENT_PROFILE")? != "codex"
        || command.args.len() != 2
        || command.args[0] != "-lic"
        || !command.args[1].starts_with("exec codex ")
        || command.args[1].contains(" --remote ")
    {
        return None;
    }
    let process = env(norma::agent_hook::ENV_CONTROL_RUN_ID)?;
    if process.is_empty() || process.len() > 128 {
        return None;
    }
    Some((env("MULTISOUL_NODE_ID")?.parse().ok()?, process.into()))
}

fn launch_commands(
    original: &PtyCommand,
    server: &Path,
    relay: &Path,
) -> Result<(PtyCommand, PtyCommand), PtyError> {
    let script = original
        .args
        .get(1)
        .and_then(|s| s.strip_prefix("exec codex "))
        .ok_or_else(|| PtyError::Spawn("unsupported native Codex command".into()))?;
    let quote = crate::session_snapshot::shell_quote;
    let mut service = original.clone();
    service.args[1] = format!(
        "exec codex{} app-server --listen {}",
        if script.contains("--dangerously-bypass-hook-trust") {
            " --dangerously-bypass-hook-trust"
        } else {
            ""
        },
        quote(&format!("unix://{}", server.display()))
    );
    let mut native = original.clone();
    native.args[1] = format!(
        "exec codex --remote {} {script}",
        quote(&format!("unix://{}", relay.display()))
    );
    Ok((service, native))
}

pub(super) fn prepare(
    id: TerminalSessionId,
    command: PtyCommand,
    cols: u16,
    rows: u16,
    control: Arc<LaunchControl>,
) -> Result<PreparedAgentTerminal, PtyError> {
    prepare_with(
        id,
        command,
        cols,
        rows,
        control,
        deferred::Limits::default(),
    )
}

pub(super) fn prepare_with(
    id: TerminalSessionId,
    command: PtyCommand,
    cols: u16,
    rows: u16,
    control: Arc<LaunchControl>,
    limits: deferred::Limits,
) -> Result<PreparedAgentTerminal, PtyError> {
    let (node, process) =
        owned_identity(&command).ok_or_else(|| PtyError::Spawn("question owner missing".into()))?;
    let backend = deferred::DeferredPty::with_limits(control.clone(), cols, rows, limits);
    let slot = backend.preparation();
    let (sender, receiver) = crossbeam_channel::unbounded();
    let (host_sender, host_receiver) = crossbeam_channel::bounded(1);
    std::thread::Builder::new()
        .name("codex-native-launch".into())
        .spawn(move || {
            let result = launch(command, node, process, &slot, &control, sender.clone());
            match result {
                Ok(host) if !control.is_cancelled() => {
                    if let Some(host) = host {
                        if host_sender
                            .send(crate::live::NativeQuestionHost::Codex(host))
                            .is_err()
                        {
                            control.cancel();
                        }
                    }
                    drop(host_sender);
                }
                Ok(_) => control.cancel(),
                Err(error) => {
                    // Already cancelled: the TUI exited (its own `Exited` was forwarded) or the card
                    // was closed. Only a genuine launch failure reports and ends the session here.
                    if !control.is_cancelled() {
                        let message: String = error
                            .to_string()
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(1000)
                            .collect();
                        let _ = sender.send(PtyEvent::Output(
                            format!("\r\nCodex 启动失败：{message}\r\n").into_bytes(),
                        ));
                        let _ = sender.send(PtyEvent::Exited { code: Some(1) });
                    }
                    control.cancel();
                }
            }
        })?;
    Ok(PreparedAgentTerminal {
        id,
        cols,
        rows,
        backend: Box::new(backend),
        receiver,
        host: Some(host_receiver),
    })
}

fn launch(
    command: PtyCommand,
    node: u64,
    process: String,
    slot: &deferred::Preparation,
    control: &Arc<LaunchControl>,
    output: crossbeam_channel::Sender<PtyEvent>,
) -> Result<Option<CodexQuestionHost>, PtyError> {
    check_launch(control)?;
    let directory = tempfile::Builder::new()
        .prefix("norma-question-")
        .tempdir_in("/tmp")?;
    let server_path = directory.path().join("server.sock");
    let relay_path = directory.path().join("native.sock");
    let listener = UnixListener::bind(&relay_path)?;
    listener.set_nonblocking(true)?;
    let (server, mut native) = launch_commands(&command, &server_path, &relay_path)?;
    let binding = directory.path().join("version");
    let quote = crate::session_snapshot::shell_quote;
    // Probe in the same shell/environment that will exec the original TUI. Unknown
    // versions retain the original command, cwd and resume; no server is launched.
    native.args[1] = format!(
        r#"
norma_codex_version=$(codex --version 2>/dev/null)
if [[ "$norma_codex_version" != 'codex-cli 0.162.1' ]]; then
    printf 'unsupported' > {binding}
    printf '{marker}'
    {original}
fi
printf 'supported' > {binding}
while [[ ! -S {server_path} ]]; do sleep 0.02; done
printf '{marker}'
{remote}
"#,
        binding = quote(&binding.to_string_lossy()),
        server_path = quote(&server_path.to_string_lossy()),
        original = command.args[1],
        remote = native.args[1],
        marker = r"\033]9999;norma-tui-start\007",
    );
    let (sender, receiver) = crossbeam_channel::bounded(64);
    let (cols, rows) = slot.dimensions();
    let native_pty = control.spawn_pty(native, cols, rows, sender)?;
    // Install immediately: the native TUI may show an interactive prompt (e.g. "Update available")
    // before it connects, and the user must be able to answer it. Input stays gated on visible output.
    slot.install(native_pty)?;
    // Resizes must reach the TUI during the handshake too, or the prompt draws at a stale size.
    let sizes = slot.clone();
    std::thread::Builder::new()
        .name("codex-native-resize".into())
        .spawn(move || sizes.maintain_sizes())?;
    let visible = slot.clone();
    let exit_control = control.clone();
    std::thread::Builder::new()
        .name("codex-native-output".into())
        .spawn(move || {
            while let Ok(event) = receiver.recv() {
                let exited = matches!(event, PtyEvent::Exited { .. });
                if let PtyEvent::Output(bytes) = &event {
                    visible.observe_output(bytes);
                }
                if output.send(event).is_err() || exited {
                    break;
                }
            }
            exit_control.cancel();
        })?;
    loop {
        check_launch(control)?;
        match std::fs::read(&binding).as_deref() {
            Ok(b"unsupported") => {
                return Ok(None);
            }
            Ok(b"supported") => break,
            _ => wait_launch(control, slot)?,
        }
    }
    control.spawn_native_host(server, directory)?;
    while !server_path.exists() {
        wait_launch(control, slot)?;
    }
    let native_socket = loop {
        check_launch(control)?;
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                wait_launch(control, slot)?
            }
            Err(error) => return Err(error.into()),
        }
    };
    let relay = CodexQuestionRelay::connect_verified(native_socket, &server_path)
        .map_err(PtyError::Spawn)?;
    let (connections, incoming) = crossbeam_channel::bounded(8);
    let host = CodexQuestionHost::start_with_connections(relay, incoming, node, process)
        .map_err(PtyError::Spawn)?;
    let accept_control = control.clone();
    std::thread::Builder::new()
        .name("codex-native-connections".into())
        .spawn(move || {
            while !accept_control.is_cancelled() {
                match listener.accept() {
                    Ok((socket, _)) => {
                        match CodexQuestionRelay::connect_verified_paused(socket, &server_path) {
                            Ok(relay) => {
                                if connections.send(relay).is_err() {
                                    break;
                                }
                            }
                            Err(reason) => {
                                tracing::warn!(%reason, "native Codex connection failed")
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => {
                        tracing::warn!(%error, "native Codex listener failed");
                        break;
                    }
                }
            }
        })?;
    Ok(Some(host))
}

fn check_launch(control: &LaunchControl) -> Result<(), PtyError> {
    if control.is_cancelled() {
        return Err(PtyError::Spawn("native launch cancelled".into()));
    }
    Ok(())
}
fn wait_launch(control: &LaunchControl, slot: &deferred::Preparation) -> Result<(), PtyError> {
    check_launch(control)?;
    if slot.launch_expired() {
        return Err(PtyError::Spawn("native Codex startup timed out".into()));
    }
    std::thread::sleep(Duration::from_millis(10));
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/codex_native_handshake_tests.rs"]
mod handshake_tests;
#[cfg(test)]
#[path = "../../../tests/unit/codex_native_launch_tests.rs"]
mod tests;
