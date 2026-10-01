use super::{MobileServer, Shared};
use anyhow::{ensure, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures::{SinkExt, StreamExt};
use norma::mobile::RelaySession;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use tokio::{sync::Notify, time::timeout};
use tokio_util::sync::CancellationToken;
use tungstenite::Message;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
// Same envelope budget as the Node relay: base64(256 KiB) + at most 1 KiB metadata.
const MAX_FRAME: usize = 4 * (256 * 1024usize).div_ceil(3) + 1024;

#[cfg(all(test, debug_assertions))]
#[path = "../../../tests/unit/mobile_relay_configuration_tests.rs"]
mod configuration_tests;

pub(super) struct Control {
    pub cancelled: CancellationToken,
    pub changed: Arc<Notify>,
}

impl MobileServer {
    pub fn start_collaboration_relay(&self) -> Result<()> {
        #[cfg(debug_assertions)]
        {
            // Development builds stay local; a missing/invalid port never contacts production.
            match std::env::var("NORMA_CLOUD_LOCAL_PORT") {
                Ok(port) => start(self.shared.clone(), local_audience(&port)?),
                Err(std::env::VarError::NotPresent) => Ok(()),
                Err(_) => anyhow::bail!("invalid local relay port"),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            start(
                self.shared.clone(),
                "https://soulvoiceai.com/norma/voice".into(),
            )
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn start_local_collaboration_relay_for_e2e(&self, port: u16) -> Result<()> {
        start(self.shared.clone(), local_audience(&port.to_string())?)
    }
}

#[cfg(any(debug_assertions, feature = "e2e-test-support"))]
fn local_audience(port: &str) -> Result<String> {
    ensure!(
        !port.is_empty() && port.bytes().all(|value| value.is_ascii_digit()),
        "invalid local relay port"
    );
    let port: u16 = port.parse()?;
    ensure!(port != 0, "invalid local relay port");
    Ok(format!("http://127.0.0.1:{port}"))
}

fn start(shared: Arc<Shared>, audience: String) -> Result<()> {
    let runtime = crate::app::shared_tokio_runtime().map_err(anyhow::Error::msg)?;
    let mut control = shared
        .relay
        .lock()
        .map_err(|_| anyhow::anyhow!("relay lock"))?;
    ensure!(
        !shared.stopped.load(Ordering::Acquire),
        "mobile service stopped"
    );
    ensure!(control.is_none(), "relay already started");
    let cancelled = CancellationToken::new();
    let changed = Arc::new(Notify::new());
    *control = Some(Control {
        cancelled: cancelled.clone(),
        changed: changed.clone(),
    });
    drop(control);
    runtime.spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = cancelled.cancelled() => break,
                _ = changed.notified() => {},
                _ = connected(shared.clone(), &audience) => {},
            }
            // Reconnect the transport only. Operation retry belongs to the durable phone outbox.
            // 2s bounds registration load to 30 attempts/minute while a local relay is unavailable.
            tokio::select! {
                _ = cancelled.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {},
            }
        }
    });
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Challenge {
        challenge: String,
        audience: String,
    },
    Registered {
        machine_id: String,
    },
    Open {
        stream_id: String,
        client_id: String,
    },
    Data {
        stream_id: String,
        data: String,
    },
    Close {
        stream_id: String,
        reason: String,
    },
}

struct Peer {
    opened: Instant,
    cipher: Option<RelaySession>,
    device: Option<String>,
    terminal: crate::mobile::thread_terminal_control::Connection,
}

async fn connected(shared: Arc<Shared>, audience: &str) -> Result<()> {
    // Both callers supply a fixed trusted endpoint; no URL received from the network is followed.
    let endpoint = audience.replacen("http", "ws", 1) + "/v1/relay/mac";
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME))
        .max_frame_size(Some(MAX_FRAME));
    let (mut socket, _) = timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async_with_config(endpoint, Some(config), false),
    )
    .await??;
    let Event::Challenge {
        challenge,
        audience: actual,
    } = receive(&mut socket, 10).await?
    else {
        anyhow::bail!("relay challenge required");
    };
    ensure!(actual == audience, "relay audience mismatch");
    let owner = shared.clone();
    let registration = tokio::task::spawn_blocking(move || {
        owner
            .access()?
            .identity
            .relay_registration(&actual, &challenge)
    })
    .await??;
    send(&mut socket, registration.clone()).await?;
    let Event::Registered { machine_id } = receive(&mut socket, 10).await? else {
        anyhow::bail!("relay registration required");
    };
    ensure!(
        registration["machine_id"] == machine_id,
        "relay registration mismatch"
    );
    let mut peers: HashMap<String, Peer> = HashMap::new();
    let mut expiry = tokio::time::interval(Duration::from_secs(1));
    let mut last_received = Instant::now();
    loop {
        let event = tokio::select! {
            message = socket.next() => {
                last_received = Instant::now();
                match message.ok_or_else(|| anyhow::anyhow!("relay disconnected"))?? {
                    Message::Text(text) => serde_json::from_str::<Event>(&text)?,
                    Message::Ping(_) => { socket.flush().await?; continue; },
                    Message::Pong(_) => continue,
                    _ => anyhow::bail!("relay connection closed"),
                }
            },
            _ = expiry.tick() => {
                // 45s = relay's 40s heartbeat failure budget + 5s scheduling margin.
                ensure!(last_received.elapsed() < Duration::from_secs(45), "relay heartbeat timeout");
                let expired: Vec<_> = peers.iter().filter(|(_, peer)|
                    peer.device.is_none() && peer.opened.elapsed() >= Duration::from_secs(10))
                    .map(|(id, _)| id.clone()).collect();
                for id in expired {
                    peers.remove(&id);
                    send(&mut socket, json!({"type":"close", "stream_id":id})).await?;
                }
                continue;
            }
        };
        match event {
            Event::Open {
                stream_id,
                client_id,
            } => {
                ensure!(
                    stream_id.len() == 36
                        && client_id.len() == 36
                        && peers.len() < 8
                        && !peers.contains_key(&stream_id),
                    "invalid relay open"
                );
                peers.insert(
                    stream_id,
                    Peer {
                        opened: Instant::now(),
                        cipher: None,
                        device: None,
                        terminal: Default::default(),
                    },
                );
            }
            Event::Data { stream_id, data } => {
                let Some(peer) = peers.remove(&stream_id) else {
                    anyhow::bail!("unknown relay stream")
                };
                let id = stream_id.clone();
                let owner = shared.clone();
                let mut pending = crate::mobile::thread_terminal_control::PendingDisconnect(Some(
                    peer.terminal.scope.clone(),
                ));
                // SQLite/auth and request handling run off both GPUI and the async network executor.
                let result =
                    tokio::task::spawn_blocking(move || respond(&owner, &id, peer, &data)).await?;
                pending.0 = None;
                match result {
                    Ok((peer, bytes)) => {
                        peers.insert(stream_id.clone(), peer);
                        send(&mut socket, json!({"type":"data", "stream_id":stream_id, "data":STANDARD.encode(bytes)})).await?;
                    }
                    Err(_) => {
                        send(&mut socket, json!({"type":"close", "stream_id":stream_id})).await?
                    }
                }
            }
            Event::Close { stream_id, reason } => {
                ensure!(reason.len() <= 128, "invalid close reason");
                peers.remove(&stream_id);
            }
            _ => anyhow::bail!("unexpected relay control"),
        }
    }
}

fn respond(shared: &Arc<Shared>, id: &str, mut peer: Peer, data: &str) -> Result<(Peer, Vec<u8>)> {
    ensure!(
        peer.device.is_some() || peer.opened.elapsed() < Duration::from_secs(10),
        "relay authentication timeout"
    );
    let bytes = STANDARD.decode(data)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 256 * 1024 && STANDARD.encode(&bytes) == data,
        "invalid relay payload"
    );
    let response = if let Some(cipher) = peer.cipher.as_mut() {
        let plain = cipher.open(&bytes)?;
        let reply = super::connection::relay::handle(
            std::str::from_utf8(&plain)?,
            shared,
            &mut peer.device,
            &mut peer.terminal,
        )?;
        cipher.seal(&reply)?
    } else {
        let hello = serde_json::from_slice(&bytes)?;
        let (cipher, handshake) = shared.access()?.identity.accept_relay_session(id, hello)?;
        peer.cipher = Some(cipher);
        serde_json::to_vec(&handshake)?
    };
    Ok((peer, response))
}

async fn receive(socket: &mut Socket, seconds: u64) -> Result<Event> {
    timeout(Duration::from_secs(seconds), async {
        loop {
            match socket
                .next()
                .await
                .ok_or_else(|| anyhow::anyhow!("relay disconnected"))??
            {
                Message::Text(text) => return Ok(serde_json::from_str(&text)?),
                Message::Ping(_) => socket.flush().await?,
                Message::Pong(_) => {}
                _ => anyhow::bail!("relay connection closed"),
            }
        }
    })
    .await?
}

async fn send(socket: &mut Socket, value: Value) -> Result<()> {
    let text = value.to_string();
    ensure!(text.len() <= MAX_FRAME, "relay output too large");
    timeout(Duration::from_secs(5), socket.send(Message::text(text))).await??;
    Ok(())
}
