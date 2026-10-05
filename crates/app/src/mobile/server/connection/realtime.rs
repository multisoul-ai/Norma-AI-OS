use super::{failure, success, Action, Request, Shared};
use norma::mobile::MobileSocket;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tungstenite::Message;
mod events;
mod execution;
mod policy;
mod sideband;

pub(super) fn operations(
    shared: &Shared,
    machine: &str,
    device: &str,
    token: &str,
    workspace: &str,
    after: u64,
) -> Result<serde_json::Value, &'static str> {
    let access = shared.access().map_err(|_| "unavailable")?;
    let grant = access
        .authenticate(machine, device, token)
        .map_err(|_| "unavailable")?
        .ok_or("unauthorized")?;
    if !grant.allows(workspace, false) {
        return Err("unauthorized");
    }
    let store = storage::MobileCommandStore::new(access.store.path().to_path_buf());
    let items = store
        .realtime_operations(device, workspace, after, &shared.owner_id)
        .map_err(|_| "unavailable")?;
    Ok(json!({"status":"realtime_operations","grant":grant,"operations":items}))
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::mobile) struct Binding {
    pub call_id: String,
    pub workspace_id: String,
    pub generation: u64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::mobile) struct Gateway {
    url: String,
    token: String,
    expires_at: u64,
}

impl Gateway {
    fn request(&self, call: &str) -> Result<tungstenite::handshake::client::Request, &'static str> {
        use tungstenite::client::IntoClientRequest;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "invalid_request")?
            .as_secs();
        let mut url = url::Url::parse(&self.url).map_err(|_| "invalid_request")?;
        let local = cfg!(feature = "e2e-test-support")
            && !cfg!(feature = "production-data")
            && url.scheme() == "ws"
            && matches!(url.host_str(), Some("127.0.0.1" | "[::1]"));
        let tools_path = url.path().strip_suffix("/v1/tools").is_some_and(|prefix| {
            prefix.split('/').skip(1).all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            })
        });
        if (url.scheme() != "wss" && !local)
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !tools_path
            || self.expires_at <= now
            || self.expires_at > now.saturating_add(60)
            || self.token.len() != 43
            || !self
                .token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return Err("invalid_request");
        }
        // A 30s gateway ticket permits up to 30s clock skew; the gateway enforces its own expiry.
        url.query_pairs_mut().append_pair("call_id", call);
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|_| "invalid_request")?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", self.token)
                .parse()
                .map_err(|_| "invalid_request")?,
        );
        Ok(request)
    }
}

pub(in crate::mobile::server) struct Session {
    binding: Binding,
    gateway: Option<Gateway>,
    machine: String,
    device: String,
    token: String,
    grant_revision: u64,
    snapshot_observed_at: AtomicU64,
    cancelled: CancellationToken,
    state: Mutex<(&'static str, Option<&'static str>)>,
}

impl Session {
    fn state(&self, state: &'static str, error: Option<&'static str>) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = (state, error);
        crate::mobile::project_events::changed();
    }
}

struct Guard(Arc<Session>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.cancelled.cancel();
    }
}

pub(super) fn serve(socket: &mut MobileSocket, shared: &Arc<Shared>, id: u64, request: Request) {
    if let Err(code) = run(socket, shared, id, &request) {
        let _ = socket.send(Message::text(
            failure(&request.request_id, code).to_string(),
        ));
    }
}

fn run(
    socket: &mut MobileSocket,
    shared: &Arc<Shared>,
    id: u64,
    request: &Request,
) -> Result<(), &'static str> {
    let Action::BindRealtime {
        device_id,
        token,
        binding,
        gateway,
    } = &request.action
    else {
        return Err("invalid_request");
    };
    let media_valid = if gateway.is_some() {
        binding.call_id.starts_with("doubao_") && binding.call_id.len() > 7
    } else {
        binding.call_id.starts_with("rtc_") && binding.call_id.len() > 4
    };
    if !media_valid
        || !crate::mobile::command::valid_id(&binding.call_id)
        || !crate::mobile::command::valid_id(&binding.workspace_id)
        || binding.generation == 0
    {
        return Err("invalid_request");
    }
    if let Some(gateway) = gateway {
        gateway.request(&binding.call_id)?;
    }
    let session = {
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&request.machine_id, device_id, token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if !grant.allows(&binding.workspace_id, true) {
            return Err("unauthorized");
        }
        // Reuse the eight existing socket slots. Register before any snapshot/database/network wait.
        // Revocation takes these same locks in the same order.
        let mut sockets = shared.sockets.lock().map_err(|_| "unavailable")?;
        for (other_id, other) in sockets.iter() {
            if let Some(old) = &other.realtime {
                if old.binding.call_id == binding.call_id && old.device != *device_id {
                    return Err("session_in_use");
                }
                if old.device == *device_id
                    && (*other_id > id
                        || (old.binding.call_id == binding.call_id
                            && old.binding.generation >= binding.generation))
                {
                    return Err("stale_binding");
                }
            }
        }
        for other in sockets.values_mut() {
            if other.device_id.as_deref() == Some(device_id) {
                if let Some(old) = other.realtime.take() {
                    old.cancelled.cancel();
                }
            }
        }
        let session = Arc::new(Session {
            binding: binding.clone(),
            gateway: gateway.clone(),
            machine: request.machine_id.clone(),
            device: device_id.clone(),
            token: token.clone(),
            grant_revision: grant.revision,
            snapshot_observed_at: AtomicU64::new(0),
            cancelled: CancellationToken::new(),
            state: Mutex::new(("connecting", None)),
        });
        let active = sockets.get_mut(&id).ok_or("unavailable")?;
        active.device_id = Some(device_id.clone());
        active.realtime = Some(Arc::clone(&session));
        session
    };
    let _guard = Guard(Arc::clone(&session));
    let runtime = crate::app::shared_tokio_runtime().map_err(|_| "unavailable")?;
    let worker = Arc::clone(&session);
    let owner = Arc::clone(shared);
    runtime.spawn(async move {
        let result = tokio::select! {
            _ = worker.cancelled.cancelled() => return,
            result = sideband::run(owner, Arc::clone(&worker)) => result,
        };
        worker.state(
            "unavailable",
            Some(result.err().unwrap_or("sideband_disconnected")),
        );
    });
    let waiter = super::watch_wait::WatchWait::new(socket)?;
    loop {
        if session.cancelled.is_cancelled() {
            return Err("stale_binding");
        }
        let revision = crate::mobile::project_events::revision();
        let access = shared.access().map_err(|_| "unavailable")?;
        let grant = access
            .authenticate(&session.machine, &session.device, &session.token)
            .map_err(|_| "unauthorized")?
            .ok_or("unauthorized")?;
        if grant.revision != session.grant_revision || !grant.allows(&binding.workspace_id, true) {
            return Err("unauthorized");
        }
        drop(access);
        let (state, error) = *session.state.lock().map_err(|_| "unavailable")?;
        let observed = session.snapshot_observed_at.load(Ordering::Relaxed);
        let value = success(
            &request.request_id,
            json!({"status":"realtime_binding", "binding":binding,
            "machine_id":session.machine, "state":state, "error":error, "grant":grant,
            "snapshot_observed_at_unix_ms":(observed > 0).then_some(observed)}),
        );
        socket
            .get_mut()
            .sock
            .set_deadline(Instant::now() + std::time::Duration::from_secs(3));
        socket
            .send(Message::text(value.to_string()))
            .map_err(|_| "unavailable")?;
        if state == "unavailable" {
            return Ok(());
        }
        waiter.wait(socket, shared, revision, Instant::now())?;
    }
}
