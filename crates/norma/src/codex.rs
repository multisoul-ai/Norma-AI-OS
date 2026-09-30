//! One private app-server connection. The caller owns the server process separately.
use crossbeam_channel::{bounded, Receiver, Sender};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::{protocol::WebSocketConfig, Error, Message, WebSocket};

mod approvals;
pub use approvals::{CodexApproval, CodexApprovalChange, CodexApprovalState, CodexApprovals};

type Reply = Result<Value, String>;
type Pending = HashMap<String, (Sender<Reply>, Instant)>;
// Bound concurrent RPCs and unread events; overload disconnects instead of losing approvals.
const CAPACITY: usize = 128;
// Large histories must be paginated; an oversized message fails explicitly, never truncates.
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

enum Command {
    Request(String, Value, Sender<Reply>),
    Response(Value, Value),
}

pub struct CodexConnection {
    commands: Sender<Command>,
    events: Receiver<Value>,
    connected: Arc<AtomicBool>,
}

impl CodexConnection {
    /// Blocking handshake: call only on a background executor.
    pub fn connect(path: &Path) -> Result<Self, String> {
        let stream = UnixStream::connect(path).map_err(|error| error.to_string())?;
        let timeout = Some(Duration::from_secs(5));
        stream
            .set_read_timeout(timeout)
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(timeout)
            .map_err(|e| e.to_string())?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES))
            .write_buffer_size(0)
            .max_write_buffer_size(MAX_MESSAGE_BYTES + 1024);
        let (mut socket, _) =
            tungstenite::client::client_with_config("ws://localhost/", stream, Some(config))
                .map_err(|error| error.to_string())?;
        let (event_tx, events) = bounded(CAPACITY);
        socket.send(Message::Text(json!({
            "id": "norma-initialize", "method": "initialize", "params": {
                "clientInfo": {"name": "norma_mobile_control", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": true}
            }
        }).to_string().into())).map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() >= deadline {
                return Err("Codex initialization timed out".into());
            }
            let frame = socket.read().map_err(|error| error.to_string())?;
            let Message::Text(text) = frame else {
                if matches!(frame, Message::Close(_)) {
                    return Err("Codex closed during initialization".into());
                }
                continue;
            };
            let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
            if value.get("method").is_none() && value["id"] == "norma-initialize" {
                if let Some(error) = value.get("error") {
                    return Err(error.to_string());
                }
                if value.get("result").is_none() {
                    return Err("Invalid Codex initialize response".into());
                }
                break;
            }
            event_tx
                .try_send(value)
                .map_err(|_| "Codex initialization event overflow")?;
        }
        socket
            .send(Message::Text(
                json!({"method":"initialized"}).to_string().into(),
            ))
            .map_err(|error| error.to_string())?;
        socket
            .get_mut()
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let (commands, command_rx) = bounded(CAPACITY);
        let connected = Arc::new(AtomicBool::new(true));
        let status = connected.clone();
        std::thread::Builder::new()
            .name("codex-control".into())
            .spawn(move || run(socket, command_rx, event_tx, status))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            commands,
            events,
            connected,
        })
    }

    /// Acceptance means queued locally. Only the returned RPC result confirms server receipt.
    pub fn request(&self, method: &str, params: Value) -> Result<Receiver<Reply>, String> {
        let (sender, receiver) = bounded(1);
        self.enqueue(Command::Request(method.into(), params, sender))?;
        Ok(receiver)
    }

    /// Preserve the Provider request ID. A queued response is not proof the tool executed.
    pub fn respond(&self, id: Value, result: Value) -> Result<(), String> {
        if !id.is_string() && !id.is_number() {
            return Err("Invalid Codex request ID".into());
        }
        self.enqueue(Command::Response(id, result))
    }

    pub fn events(&self) -> &Receiver<Value> {
        &self.events
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn enqueue(&self, command: Command) -> Result<(), String> {
        if !self.is_connected() {
            return Err("Codex disconnected; command was not queued".into());
        }
        self.commands
            .try_send(command)
            .map_err(|_| "Codex unavailable or command queue full".into())
    }
}

impl Drop for CodexConnection {
    fn drop(&mut self) {
        // No join/IO on the UI thread; the worker checks this every polling interval.
        self.connected.store(false, Ordering::Release);
    }
}

fn run(
    mut socket: WebSocket<UnixStream>,
    commands: Receiver<Command>,
    events: Sender<Value>,
    connected: Arc<AtomicBool>,
) {
    let mut pending = Pending::new();
    let outcome = pump(&mut socket, &commands, &events, &connected, &mut pending);
    connected.store(false, Ordering::Release);
    let reason = outcome
        .err()
        .unwrap_or_else(|| "Codex connection closed".into());
    for (_, (reply, _)) in pending {
        let _ = reply.try_send(Err(format!(
            "{reason}; request outcome may be unknown, do not replay"
        )));
    }
    for command in commands.try_iter() {
        if let Command::Request(_, _, reply) = command {
            let _ = reply.try_send(Err("Codex disconnected before sending request".into()));
        }
    }
}

fn pump(
    socket: &mut WebSocket<UnixStream>,
    commands: &Receiver<Command>,
    events: &Sender<Value>,
    connected: &AtomicBool,
    pending: &mut Pending,
) -> Result<(), String> {
    let mut next_id = 0_u64;
    while connected.load(Ordering::Acquire) {
        let mut active = false;
        if pending
            .values()
            .any(|(_, deadline)| Instant::now() >= *deadline)
        {
            return Err("Codex RPC timed out".into());
        }
        // Fair bounded batches ensure a flood cannot starve replies, timeout or shutdown.
        for command in commands.try_iter().take(32) {
            active = true;
            let value = match command {
                Command::Request(method, params, reply) => {
                    if pending.len() >= CAPACITY {
                        let _ =
                            reply.try_send(Err("Too many Codex requests; request not sent".into()));
                        continue;
                    }
                    next_id = next_id.checked_add(1).ok_or("Codex RPC IDs exhausted")?;
                    let id = format!("norma-{next_id}");
                    pending.insert(id.clone(), (reply, Instant::now() + RPC_TIMEOUT));
                    json!({"id":id,"method":method,"params":params})
                }
                Command::Response(id, result) => json!({"id":id,"result":result}),
            };
            let encoded = value.to_string();
            if encoded.len() > MAX_MESSAGE_BYTES {
                return Err("Codex outgoing message too large".into());
            }
            // WouldBlock has already buffered this frame: flush it, never enqueue it twice.
            tolerate_would_block(socket.send(Message::Text(encoded.into())))?;
        }
        tolerate_would_block(socket.flush())?;
        for _ in 0..32 {
            match socket.read() {
                Ok(Message::Text(text)) => {
                    active = true;
                    let value: Value =
                        serde_json::from_str(&text).map_err(|error| error.to_string())?;
                    if value.get("method").is_some() {
                        events
                            .try_send(value)
                            .map_err(|_| "Codex event gap: consumer unavailable or queue full")?;
                    } else if let Some(id) = value["id"].as_str() {
                        if let Some((reply, _)) = pending.remove(id) {
                            let result = if let Some(error) = value.get("error") {
                                Err(error.to_string())
                            } else {
                                value
                                    .get("result")
                                    .cloned()
                                    .ok_or_else(|| "Invalid Codex response".into())
                            };
                            let _ = reply.try_send(result);
                        }
                    } else {
                        return Err("Invalid Codex RPC frame".into());
                    }
                }
                Ok(Message::Close(_)) => return Err("Codex server closed".into()),
                Ok(Message::Ping(_) | Message::Pong(_)) => {}
                Ok(_) => return Err("Codex sent a non-JSON frame".into()),
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.to_string()),
            }
        }
        if !active {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

fn tolerate_would_block(result: Result<(), Error>) -> Result<(), String> {
    match result {
        Ok(()) => Ok(()),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
