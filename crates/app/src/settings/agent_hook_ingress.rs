use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use gpui::{App, Global};
use norma::agent_hook::{fresh_control_id, AgentHookEvent, HookApproval};
mod approval_wait;
mod http;
use approval_wait::WaitingHook;
pub(super) use http::AgentHookHttpError;
use http::{read_hook_request, write_status, HookRequest};
use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CLIENT_RECV_INTERVAL: Duration = Duration::from_millis(25);
const STREAM_TIMEOUT: Duration = Duration::from_secs(2);
const CLIENT_QUEUE_CAPACITY: usize = 32;
const EVENT_QUEUE_CAPACITY: usize = 256;
const CLIENT_WORKER_COUNT: usize = 4;

#[cfg(test)]
#[path = "../../tests/unit/agent_hook_approval_transport_tests.rs"]
mod approval_transport_tests;

pub(crate) struct AgentHookRegistration {
    pub(crate) endpoint: Option<String>,
    pub(crate) token: Option<String>,
    pub(crate) receiver: Receiver<AgentHookEvent>,
    pub(crate) approvals: Receiver<HookApproval>,
    #[cfg(feature = "e2e-test-support")]
    pub(crate) sender: Sender<AgentHookEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HookDelivery {
    Accepted,
    Unauthorized,
    Retired,
    Overloaded,
}

#[derive(Default)]
struct AgentHookRegistry {
    routes: HashMap<String, HookRoute>,
    retired_tokens: HashSet<String>,
}

struct HookRoute {
    events: Sender<AgentHookEvent>,
    approvals: Sender<HookApproval>,
}

impl AgentHookRegistry {
    fn register(
        &mut self,
    ) -> std::io::Result<(
        String,
        Receiver<AgentHookEvent>,
        Sender<AgentHookEvent>,
        Receiver<HookApproval>,
    )> {
        let (sender, receiver) = bounded(EVENT_QUEUE_CAPACITY);
        let (approvals_sender, approvals) = bounded(approval_wait::CAPACITY);
        let mut token = fresh_control_id()?;
        while self.routes.contains_key(&token) || self.retired_tokens.contains(&token) {
            token = fresh_control_id()?;
        }
        self.routes.insert(
            token.clone(),
            HookRoute {
                events: sender.clone(),
                approvals: approvals_sender,
            },
        );
        Ok((token, receiver, sender, approvals))
    }

    fn unregister(&mut self, token: &str) -> bool {
        let removed = self.routes.remove(token).is_some();
        if removed {
            self.retired_tokens.insert(token.to_owned());
        }
        removed
    }

    fn deliver(&self, token: &str, event: AgentHookEvent) -> HookDelivery {
        let Some(sender) = self.routes.get(token) else {
            return if self.retired_tokens.contains(token) {
                HookDelivery::Retired
            } else {
                HookDelivery::Unauthorized
            };
        };
        match sender.events.try_send(event) {
            Ok(()) => HookDelivery::Accepted,
            Err(TrySendError::Full(_)) => HookDelivery::Overloaded,
            Err(TrySendError::Disconnected(_)) => HookDelivery::Retired,
        }
    }
}

pub(crate) struct AgentHookIngress {
    endpoint: String,
    registry: Arc<Mutex<AgentHookRegistry>>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Global for AgentHookIngress {}

impl Drop for AgentHookIngress {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Ok(address) = self
            .endpoint
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .parse()
        {
            let _ = TcpStream::connect_timeout(&address, ACCEPT_POLL_INTERVAL);
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("Agent Hook ingress thread panicked during shutdown");
            }
        }
    }
}

pub(crate) fn install(cx: &mut App) -> std::io::Result<()> {
    if cx.has_global::<AgentHookIngress>() {
        return Ok(());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let endpoint = format!("http://{address}/agent-hooks");
    let registry = Arc::new(Mutex::new(AgentHookRegistry::default()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let worker_registry = Arc::clone(&registry);
    let worker_shutdown = Arc::clone(&shutdown);
    let worker = thread::Builder::new()
        .name("agent-hook-ingress".to_string())
        .spawn(move || accept_loop(listener, worker_registry, worker_shutdown))?;
    cx.set_global(AgentHookIngress {
        endpoint,
        registry,
        shutdown,
        worker: Some(worker),
    });
    Ok(())
}

pub(crate) fn register(cx: &mut App) -> AgentHookRegistration {
    if let Err(error) = install(cx) {
        tracing::warn!(%error, "failed to start App-global Agent Hook ingress");
        let (sender, receiver) = bounded(EVENT_QUEUE_CAPACITY);
        #[cfg(not(feature = "e2e-test-support"))]
        let _ = &sender;
        return AgentHookRegistration {
            endpoint: None,
            token: None,
            receiver,
            approvals: bounded(1).1,
            #[cfg(feature = "e2e-test-support")]
            sender,
        };
    }
    let ingress = cx.global_mut::<AgentHookIngress>();
    let registration = ingress
        .registry
        .lock()
        .expect("Agent Hook registry mutex poisoned")
        .register();
    let Ok((token, receiver, sender, approvals)) = registration else {
        tracing::error!("secure Hook token generation failed; control disabled");
        let (sender, receiver) = bounded(EVENT_QUEUE_CAPACITY);
        #[cfg(not(feature = "e2e-test-support"))]
        let _ = &sender;
        return AgentHookRegistration {
            endpoint: None,
            token: None,
            receiver,
            approvals: bounded(1).1,
            #[cfg(feature = "e2e-test-support")]
            sender,
        };
    };
    #[cfg(not(feature = "e2e-test-support"))]
    let _ = &sender;
    AgentHookRegistration {
        endpoint: Some(ingress.endpoint.clone()),
        token: Some(token),
        receiver,
        approvals,
        #[cfg(feature = "e2e-test-support")]
        sender,
    }
}

pub(crate) fn unregister(token: &str, cx: &mut App) -> bool {
    if !cx.has_global::<AgentHookIngress>() {
        return false;
    }
    cx.global_mut::<AgentHookIngress>()
        .registry
        .lock()
        .expect("Agent Hook registry mutex poisoned")
        .unregister(token)
}

pub(crate) fn health(cx: &App) -> &'static str {
    if cx.has_global::<AgentHookIngress>() {
        "ready"
    } else {
        "unavailable"
    }
}

fn accept_loop(
    listener: TcpListener,
    registry: Arc<Mutex<AgentHookRegistry>>,
    shutdown: Arc<AtomicBool>,
) {
    let (stream_sender, stream_receiver) = bounded(CLIENT_QUEUE_CAPACITY);
    let (approval_sender, approval_receiver) = bounded(approval_wait::CAPACITY);
    let wait_registry = Arc::clone(&registry);
    let wait_shutdown = Arc::clone(&shutdown);
    let waiter = thread::Builder::new()
        .name("agent-hook-permissions".into())
        .spawn(move || approval_wait::run(approval_receiver, wait_registry, wait_shutdown));
    let Ok(waiter) = waiter else {
        return;
    };
    let workers = spawn_client_workers(
        stream_receiver,
        registry,
        Arc::clone(&shutdown),
        approval_sender,
    );
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => match stream_sender.try_send(stream) {
                Ok(()) => {}
                Err(TrySendError::Full(returned)) => {
                    stream = returned;
                    write_status(&mut stream, "503 Service Unavailable");
                }
                Err(TrySendError::Disconnected(_)) => break,
            },
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(error) => {
                tracing::warn!(%error, "Agent Hook accept failed");
                break;
            }
        }
    }
    shutdown.store(true, Ordering::Release);
    drop(stream_sender);
    for worker in workers {
        if worker.join().is_err() {
            tracing::warn!("Agent Hook client worker panicked during shutdown");
        }
    }
    let _ = waiter.join();
}

fn spawn_client_workers(
    receiver: Receiver<TcpStream>,
    registry: Arc<Mutex<AgentHookRegistry>>,
    shutdown: Arc<AtomicBool>,
    approvals: Sender<WaitingHook>,
) -> Vec<JoinHandle<()>> {
    (0..CLIENT_WORKER_COUNT)
        .filter_map(|index| {
            let streams = receiver.clone();
            let approvals = approvals.clone();
            let routes = Arc::clone(&registry);
            let worker_shutdown = Arc::clone(&shutdown);
            thread::Builder::new()
                .name(format!("agent-hook-client-{index}"))
                .spawn(move || client_worker_loop(streams, routes, worker_shutdown, approvals))
                .map_err(|error| tracing::warn!(%error, index, "failed to start Agent Hook client worker"))
                .ok()
        })
        .collect()
}

fn client_worker_loop(
    receiver: Receiver<TcpStream>,
    registry: Arc<Mutex<AgentHookRegistry>>,
    shutdown: Arc<AtomicBool>,
    approvals: Sender<WaitingHook>,
) {
    while !shutdown.load(Ordering::Acquire) || !receiver.is_empty() {
        match receiver.recv_timeout(CLIENT_RECV_INTERVAL) {
            Ok(stream) => handle_stream(stream, &registry, &approvals),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle_stream(
    mut stream: TcpStream,
    registry: &Arc<Mutex<AgentHookRegistry>>,
    approvals: &Sender<WaitingHook>,
) {
    // accept may inherit O_NONBLOCK on macOS. Workers use bounded blocking reads;
    // an HTTP packet boundary must not be treated as a malformed request.
    if stream
        .set_nonblocking(false)
        .and_then(|_| stream.set_read_timeout(Some(STREAM_TIMEOUT)))
        .and_then(|_| stream.set_write_timeout(Some(STREAM_TIMEOUT)))
        .is_err()
    {
        return;
    }
    let status = match read_hook_request(&stream) {
        Ok((token, HookRequest::Approval(request))) => {
            let sender = registry
                .lock()
                .expect("Agent Hook registry mutex poisoned")
                .routes
                .get(&token)
                .map(|route| route.approvals.clone());
            let Some(sender) = sender else {
                write_status(&mut stream, "401 Unauthorized");
                return;
            };
            let Ok(approval) = HookApproval::new(request, std::time::Instant::now()) else {
                write_status(&mut stream, "400 Bad Request");
                return;
            };
            if sender.try_send(approval.clone()).is_err() {
                approval.cancel();
                write_status(&mut stream, "503 Service Unavailable");
                return;
            }
            match WaitingHook::new(stream, token, approval.clone()) {
                Ok(waiting) => {
                    let _ = approvals.try_send(waiting);
                }
                Err(_) => approval.cancel(),
            }
            return;
        }
        Ok((token, HookRequest::Event(event))) => match registry
            .lock()
            .expect("Agent Hook registry mutex poisoned")
            .deliver(&token, event)
        {
            HookDelivery::Accepted => "202 Accepted",
            HookDelivery::Unauthorized => "401 Unauthorized",
            HookDelivery::Retired => "410 Gone",
            HookDelivery::Overloaded => "503 Service Unavailable",
        },
        Err(AgentHookHttpError::Unauthorized) => "401 Unauthorized",
        Err(_) => "400 Bad Request",
    };
    write_status(&mut stream, status);
}

#[cfg(test)]
pub(super) fn read_authorized_event(
    stream: &TcpStream,
    expected_token: &str,
) -> Result<AgentHookEvent, AgentHookHttpError> {
    let (token, event) = read_hook_request(stream)?;
    if token != expected_token {
        return Err(AgentHookHttpError::Unauthorized);
    }
    match event {
        HookRequest::Event(event) => Ok(event),
        _ => Err(AgentHookHttpError::BadRequest),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent_hook_registry_tests.rs"]
mod tests;
