//! One owned loopback server and its original native TUI; all preparation stays off-thread.
use super::*;
use crate::runtime::LaunchControl;
use norma::opencode::{client::OpenCodeQuestionClient, native_questions::OpenCodeQuestionHost};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminal::PtyEvent;

const EXEC: &str = "exec \"${OPENCODE_BIN:-opencode}\"";

pub(super) fn owned_identity(command: &PtyCommand) -> Option<(u64, String, Option<String>)> {
    let env = |key: &str| {
        command
            .env
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    if env("MULTISOUL_AGENT_PROFILE")? != "opencode"
        || command.args.len() != 2
        || command.args[0] != "-lic"
    {
        return None;
    }
    let script = &command.args[1];
    let resume = if script == EXEC {
        None
    } else {
        let value = script
            .strip_prefix(&format!("{EXEC} resume '"))?
            .strip_suffix('\'')?;
        // OpenCode session identifiers are opaque alphanumeric IDs. Never parse arbitrary shell.
        if !value.starts_with("ses_")
            || value.len() > 4096
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return None;
        }
        Some(value.to_owned())
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
    command: PtyCommand,
    cols: u16,
    rows: u16,
    control: Arc<LaunchControl>,
) -> Result<PreparedAgentTerminal, PtyError> {
    let (node, process, resume) =
        owned_identity(&command).ok_or_else(|| PtyError::Spawn("question owner missing".into()))?;
    let backend = deferred::DeferredPty::new(control.clone(), cols, rows);
    let slot = backend.preparation();
    let (sender, receiver) = crossbeam_channel::unbounded();
    let (host_sender, host_receiver) = crossbeam_channel::bounded(1);
    std::thread::Builder::new()
        .name("opencode-native-launch".into())
        .spawn(move || {
            let result = launch(
                command,
                node,
                process,
                resume.as_deref(),
                &slot,
                &control,
                sender.clone(),
            );
            match result {
                Ok(host) if !control.is_cancelled() => {
                    if let Some(host) = host {
                        if host_sender
                            .send(crate::live::NativeQuestionHost::OpenCode(host))
                            .is_err()
                        {
                            control.cancel();
                        }
                    }
                    drop(host_sender);
                    slot.maintain_sizes();
                }
                Ok(_) => control.cancel(),
                Err(error) => {
                    if !control.is_cancelled() {
                        let message: String = error
                            .to_string()
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(500)
                            .collect();
                        let _ = sender.send(PtyEvent::Output(
                            format!("\r\nOpenCode 启动失败：{message}\r\n").into_bytes(),
                        ));
                    }
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
        host: Some(host_receiver),
    })
}

fn launch(
    command: PtyCommand,
    node: u64,
    process: String,
    resume: Option<&str>,
    slot: &deferred::Preparation,
    control: &Arc<LaunchControl>,
    output: crossbeam_channel::Sender<PtyEvent>,
) -> Result<Option<OpenCodeQuestionHost>, PtyError> {
    deferred::check_launch(control)?;
    let cwd = command
        .cwd
        .as_deref()
        .ok_or_else(|| PtyError::Spawn("OpenCode workspace missing".into()))?;
    // Attach itself resolves cwd through chdir/getcwd; use that same physical directory for HTTP.
    let directory = std::fs::canonicalize(cwd)?;
    let directory = directory
        .to_str()
        .ok_or_else(|| PtyError::Spawn("OpenCode workspace is not UTF-8".into()))?;
    let bootstrap = tempfile::Builder::new()
        .prefix("norma-opencode-")
        .tempdir()?;
    let nonce = norma::agent_hook::fresh_control_id()?;
    let route = norma::opencode::foreground::install(bootstrap.path(), &nonce)?;
    let tui_config = bootstrap.path().join("tui.json");
    let binding = bootstrap.path().join("version");
    let attach = bootstrap.path().join("attach.sh");
    let quote = crate::session_snapshot::shell_quote;
    let mut native = command.clone();
    // The same login shell owns version detection and the original TUI. Unknown versions
    // never start a server or create a replacement session, including the resume path.
    native.args[1] = format!(
        r#"
norma_opencode_version=$("${{OPENCODE_BIN:-opencode}}" --version 2>/dev/null)
if [[ "$norma_opencode_version" != '1.19.3' || -n "${{OPENCODE_TUI_CONFIG:-}}" || -n "${{OPENCODE_PURE:-}}" ]]; then
    printf 'unsupported' > {binding}
    {original}
fi
printf 'supported' > {binding}
while [[ ! -f {attach} ]]; do sleep 0.02; done
source {attach}
"#,
        binding = quote(&binding.to_string_lossy()),
        attach = quote(&attach.to_string_lossy()),
        original = command.args[1],
    );
    let (sender, receiver) = crossbeam_channel::bounded(64);
    let (cols, rows) = slot.dimensions();
    let native_pty = control.spawn_pty(native, cols, rows, sender)?;
    let exit_control = control.clone();
    std::thread::Builder::new()
        .name("opencode-native-output".into())
        .spawn(move || {
            while let Ok(event) = receiver.recv() {
                let exited = matches!(event, PtyEvent::Exited { .. });
                if output.send(event).is_err() || exited {
                    break;
                }
            }
            exit_control.cancel();
        })?;
    slot.install(native_pty)?;
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        deferred::check_launch(control)?;
        match std::fs::read(&binding).as_deref() {
            Ok(b"supported") => break,
            Ok(b"unsupported") => return Ok(None),
            _ if Instant::now() >= deadline => {
                return Err(PtyError::Spawn(
                    "OpenCode version detection timed out".into(),
                ));
            }
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let endpoint = format!("http://127.0.0.1:{port}");
    let password = norma::agent_hook::fresh_control_id()
        .map_err(|_| PtyError::Spawn("OpenCode instance identity unavailable".into()))?;
    let mut server = command
        .clone()
        .env("OPENCODE_SERVER_USERNAME", "opencode")
        .env("OPENCODE_SERVER_PASSWORD", &password);
    server.args[1] = format!("{EXEC} serve --hostname 127.0.0.1 --port {port}");
    control.spawn_native_host(server, bootstrap)?;
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        deferred::check_launch(control)?;
        if OpenCodeQuestionClient::instance_ready(&endpoint, directory, &password) {
            break;
        }
        if Instant::now() >= deadline {
            return Err(PtyError::Spawn("OpenCode service startup timed out".into()));
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    deferred::check_launch(control)?;
    let (client, supported) =
        OpenCodeQuestionClient::prepare_native_session(&endpoint, directory, resume, &password)
            .map_err(|reason| PtyError::Spawn(reason.into()))?;
    deferred::check_launch(control)?;
    if !supported {
        return Err(PtyError::Spawn(
            "OpenCode server version changed during launch".into(),
        ));
    }
    let script = format!(
        "export OPENCODE_TUI_CONFIG={} OPENCODE_SERVER_USERNAME=opencode OPENCODE_SERVER_PASSWORD={}\n{EXEC} attach {} --dir {} --session {}",
        quote(&tui_config.to_string_lossy()),
        quote(&password),
        quote(&endpoint),
        quote(directory),
        quote(client.session_id())
    );
    // Publish the command atomically inside the private per-launch directory.
    let pending = attach.with_extension("pending");
    std::fs::write(&pending, script)?;
    std::fs::rename(pending, attach)?;
    Ok(Some(
        OpenCodeQuestionHost::start_following(client, node, process, route, nonce)
            .map_err(PtyError::Spawn)?,
    ))
}
