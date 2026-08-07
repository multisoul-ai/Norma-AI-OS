//! Process-wide Chrome Native Messaging broker for Browser auth requests.

use super::chrome_auth::{parse_cookie_export, response_request_id, ChromeAuthRequest};
use super::BrowserEvent;
use crate::browser_auth_host::{
    browser_auth_socket_path, bundled_extension_path, install_native_host_manifest,
    read_native_message, write_native_message,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const BROKER_TICK: Duration = Duration::from_millis(20);

static BROKER_COMMANDS: OnceLock<Sender<BrokerCommand>> = OnceLock::new();
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

struct BrokerCommand {
    request: ChromeAuthRequest,
    event_sender: Sender<BrowserEvent>,
}

struct PendingRequest {
    request: ChromeAuthRequest,
    event_sender: Sender<BrowserEvent>,
    deadline: Instant,
    sent: bool,
}

struct HostMessage {
    generation: u64,
    payload: Result<Vec<u8>, String>,
}

pub(crate) fn request_chrome_cookies(
    node_id: canvas::NodeId,
    url: &str,
    event_sender: Sender<BrowserEvent>,
) -> Result<(), String> {
    let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let request = ChromeAuthRequest::new(request_id, node_id, url)?;
    broker_commands()?
        .send(BrokerCommand {
            request,
            event_sender,
        })
        .map_err(|_| "Chrome auth broker was not running".to_string())
}

fn broker_commands() -> Result<&'static Sender<BrokerCommand>, String> {
    if let Some(sender) = BROKER_COMMANDS.get() {
        return Ok(sender);
    }
    let (sender, receiver) = unbounded();
    std::thread::Builder::new()
        .name("browser-auth-broker".to_string())
        .spawn(move || run_broker(receiver))
        .map_err(|error| format!("could not start Chrome auth broker: {error}"))?;
    let _ = BROKER_COMMANDS.set(sender);
    BROKER_COMMANDS
        .get()
        .ok_or_else(|| "Chrome auth broker initialization raced unexpectedly".to_string())
}

fn run_broker(commands: Receiver<BrokerCommand>) {
    let manifest_error = install_native_host_manifest().err();
    let listener = match bind_listener() {
        Ok(listener) => listener,
        Err(error) => {
            while let Ok(command) = commands.recv() {
                send_failure(
                    &command.event_sender,
                    command.request.node_id(),
                    error.clone(),
                );
            }
            return;
        }
    };
    let (host_sender, host_messages) = unbounded();
    let mut generation = 0_u64;
    let mut host_writer: Option<UnixStream> = None;
    let mut pending = HashMap::<u64, PendingRequest>::new();

    loop {
        if accept_host(&listener, &host_sender, &mut generation, &mut host_writer) {
            mark_pending_unsent(&mut pending);
        }
        while let Ok(message) = host_messages.try_recv() {
            if message.generation != generation {
                continue;
            }
            match message.payload {
                Ok(payload) => handle_host_payload(payload, &mut pending),
                Err(_) => {
                    host_writer = None;
                    mark_pending_unsent(&mut pending);
                }
            }
        }
        match commands.recv_timeout(BROKER_TICK) {
            Ok(command) => {
                let request_id = command.request.request_id();
                pending.insert(
                    request_id,
                    PendingRequest {
                        request: command.request,
                        event_sender: command.event_sender,
                        deadline: Instant::now() + REQUEST_TIMEOUT,
                        sent: false,
                    },
                );
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
        if let Some(writer) = host_writer.as_mut() {
            if send_unsent_requests(writer, &mut pending).is_err() {
                host_writer = None;
                mark_pending_unsent(&mut pending);
            }
        }
        expire_requests(&mut pending, manifest_error.as_deref());
    }
}

fn bind_listener() -> Result<UnixListener, String> {
    let socket_path = browser_auth_socket_path()?;
    let socket_dir = socket_path
        .parent()
        .ok_or_else(|| "Browser auth socket had no parent directory".to_string())?;
    std::fs::create_dir_all(socket_dir).map_err(|error| {
        format!(
            "could not create Browser auth socket directory {}: {error}",
            socket_dir.display()
        )
    })?;
    std::fs::set_permissions(socket_dir, std::fs::Permissions::from_mode(0o700)).map_err(
        |error| {
            format!(
                "could not restrict Browser auth socket directory {}: {error}",
                socket_dir.display()
            )
        },
    )?;
    if socket_path.exists() {
        if UnixStream::connect(&socket_path).is_ok() {
            return Err("another Norma OS process already owns the Chrome auth bridge".to_string());
        }
        std::fs::remove_file(&socket_path).map_err(|error| {
            format!(
                "could not remove stale Browser auth socket {}: {error}",
                socket_path.display()
            )
        })?;
    }
    let listener = UnixListener::bind(&socket_path).map_err(|error| {
        format!(
            "could not bind Browser auth socket {}: {error}",
            socket_path.display()
        )
    })?;
    std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600)).map_err(
        |error| {
            format!(
                "could not restrict Browser auth socket {}: {error}",
                socket_path.display()
            )
        },
    )?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("could not make Browser auth socket nonblocking: {error}"))?;
    Ok(listener)
}

fn accept_host(
    listener: &UnixListener,
    host_sender: &Sender<HostMessage>,
    generation: &mut u64,
    host_writer: &mut Option<UnixStream>,
) -> bool {
    let Ok((stream, _)) = listener.accept() else {
        return false;
    };
    let Ok(writer) = stream.try_clone() else {
        return false;
    };
    *generation = generation.wrapping_add(1);
    *host_writer = Some(writer);
    let connection_generation = *generation;
    let sender = host_sender.clone();
    let _ = std::thread::Builder::new()
        .name("browser-auth-host-reader".to_string())
        .spawn(move || read_host_messages(stream, connection_generation, sender));
    true
}

fn mark_pending_unsent(pending: &mut HashMap<u64, PendingRequest>) {
    for entry in pending.values_mut() {
        entry.sent = false;
    }
}

fn read_host_messages(mut stream: UnixStream, generation: u64, sender: Sender<HostMessage>) {
    loop {
        match read_native_message(&mut stream) {
            Ok(payload) => {
                if sender
                    .send(HostMessage {
                        generation,
                        payload: Ok(payload),
                    })
                    .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(HostMessage {
                    generation,
                    payload: Err(format!("Chrome auth host disconnected: {error}")),
                });
                return;
            }
        }
    }
}

fn send_unsent_requests(
    writer: &mut UnixStream,
    pending: &mut HashMap<u64, PendingRequest>,
) -> Result<(), String> {
    for entry in pending.values_mut().filter(|entry| !entry.sent) {
        let payload = entry.request.to_json()?;
        write_native_message(writer, &payload)
            .map_err(|error| format!("could not send request to Chrome extension: {error}"))?;
        entry.sent = true;
    }
    Ok(())
}

fn handle_host_payload(payload: Vec<u8>, pending: &mut HashMap<u64, PendingRequest>) {
    let Ok(request_id) = response_request_id(&payload) else {
        return;
    };
    let Some(entry) = pending.remove(&request_id) else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0);
    match parse_cookie_export(&payload, &entry.request, now) {
        Ok(batch) => {
            let _ = entry.event_sender.send(BrowserEvent::ChromeAuthCookies {
                node_id: entry.request.node_id(),
                url: entry.request.url().to_string(),
                batch,
            });
        }
        Err(error) => send_failure(&entry.event_sender, entry.request.node_id(), error),
    }
}

fn expire_requests(pending: &mut HashMap<u64, PendingRequest>, setup_error: Option<&str>) {
    let now = Instant::now();
    let expired = pending
        .iter()
        .filter_map(|(request_id, entry)| (entry.deadline <= now).then_some(*request_id))
        .collect::<Vec<_>>();
    for request_id in expired {
        if let Some(entry) = pending.remove(&request_id) {
            let message = setup_error
                .map(str::to_string)
                .unwrap_or_else(chrome_extension_install_guidance);
            send_failure(&entry.event_sender, entry.request.node_id(), message);
        }
    }
}

fn chrome_extension_install_guidance() -> String {
    match bundled_extension_path() {
        Ok(path) => format!(
            "Chrome session bridge did not connect. Open chrome://extensions, enable Developer mode, then Load unpacked from: {}",
            path.display()
        ),
        Err(_) => {
            "Chrome session bridge did not connect; install or enable the bundled extension"
                .to_string()
        }
    }
}

fn send_failure(sender: &Sender<BrowserEvent>, node_id: canvas::NodeId, message: String) {
    let _ = sender.send(BrowserEvent::ChromeAuthFailed { node_id, message });
}
