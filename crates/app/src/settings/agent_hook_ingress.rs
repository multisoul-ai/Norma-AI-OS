use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use gpui::{App, Global};
use norma::agent_hook::AgentHookEvent;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
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

pub(crate) struct AgentHookRegistration {
    pub(crate) endpoint: Option<String>,
    pub(crate) token: Option<String>,
    pub(crate) receiver: Receiver<AgentHookEvent>,
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
    routes: HashMap<String, Sender<AgentHookEvent>>,
    retired_tokens: HashSet<String>,
}

impl AgentHookRegistry {
    fn register(&mut self) -> (String, Receiver<AgentHookEvent>, Sender<AgentHookEvent>) {
        let (sender, receiver) = bounded(EVENT_QUEUE_CAPACITY);
        let mut token = random_token();
        while self.routes.contains_key(&token) || self.retired_tokens.contains(&token) {
            token = random_token();
        }
        self.routes.insert(token.clone(), sender.clone());
        (token, receiver, sender)
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
        match sender.try_send(event) {
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
            #[cfg(feature = "e2e-test-support")]
            sender,
        };
    }
    let ingress = cx.global_mut::<AgentHookIngress>();
    let (token, receiver, sender) = ingress
        .registry
        .lock()
        .expect("Agent Hook registry mutex poisoned")
        .register();
    #[cfg(not(feature = "e2e-test-support"))]
    let _ = &sender;
    AgentHookRegistration {
        endpoint: Some(ingress.endpoint.clone()),
        token: Some(token),
        receiver,
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
    let workers = spawn_client_workers(stream_receiver, registry, Arc::clone(&shutdown));
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
    drop(stream_sender);
    for worker in workers {
        if worker.join().is_err() {
            tracing::warn!("Agent Hook client worker panicked during shutdown");
        }
    }
}

fn spawn_client_workers(
    receiver: Receiver<TcpStream>,
    registry: Arc<Mutex<AgentHookRegistry>>,
    shutdown: Arc<AtomicBool>,
) -> Vec<JoinHandle<()>> {
    (0..CLIENT_WORKER_COUNT)
        .filter_map(|index| {
            let streams = receiver.clone();
            let routes = Arc::clone(&registry);
            let worker_shutdown = Arc::clone(&shutdown);
            thread::Builder::new()
                .name(format!("agent-hook-client-{index}"))
                .spawn(move || client_worker_loop(streams, routes, worker_shutdown))
                .map_err(|error| tracing::warn!(%error, index, "failed to start Agent Hook client worker"))
                .ok()
        })
        .collect()
}

fn client_worker_loop(
    receiver: Receiver<TcpStream>,
    registry: Arc<Mutex<AgentHookRegistry>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) || !receiver.is_empty() {
        match receiver.recv_timeout(CLIENT_RECV_INTERVAL) {
            Ok(stream) => handle_stream(stream, &registry),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle_stream(mut stream: TcpStream, registry: &Arc<Mutex<AgentHookRegistry>>) {
    let _ = stream.set_read_timeout(Some(STREAM_TIMEOUT));
    let _ = stream.set_write_timeout(Some(STREAM_TIMEOUT));
    let status = match read_hook_request(&stream) {
        Ok((token, event)) => match registry
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

fn write_status(stream: &mut TcpStream, status: &str) {
    let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn read_hook_request(stream: &TcpStream) -> Result<(String, AgentHookEvent), AgentHookHttpError> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    if !request_line.starts_with("POST /agent-hooks ") {
        return Err(AgentHookHttpError::BadRequest);
    }
    let mut token = None;
    let mut content_len = 0_usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Authorization: Bearer ") {
            token = Some(value.to_owned());
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
            content_len = value.parse::<usize>().unwrap_or(0);
        }
    }
    let token = token.ok_or(AgentHookHttpError::Unauthorized)?;
    let mut body = vec![0_u8; content_len];
    reader.read_exact(&mut body)?;
    let event = serde_json::from_slice(&body).map_err(|_| AgentHookHttpError::Json)?;
    Ok((token, event))
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
    Ok(event)
}

fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    if File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_err()
    {
        bytes = fallback_token_bytes();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fallback_token_bytes() -> [u8; 32] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ((nanos >> ((index % 16) * 8)) & 0xff) as u8;
    }
    bytes
}

#[derive(Debug)]
pub(super) enum AgentHookHttpError {
    Io,
    Json,
    Unauthorized,
    BadRequest,
}

impl From<std::io::Error> for AgentHookHttpError {
    fn from(_error: std::io::Error) -> Self {
        Self::Io
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use norma::agent_hook::{parse_hook_event, HookSource};

    /// Hook mailbox 隔离：每个 token 只能把事件送入自己的 Workspace 邮箱。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   registrations = alpha(1) + beta(1) = 2 个 token
    ///   events        = alpha/node 7(1) + beta/node 7(1) = 2 条
    ///   node collision = 两边 node_id 都为 7，刻意验证 token 隔离
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 注册 alpha 与 beta 两个独立邮箱。
    ///   2. 使用各自 token 投递同 node_id 的不同 Provider 事件。
    ///   3. 分别从两个 receiver 读取结果。
    ///
    /// 预期结果：
    ///   - 正断言：alpha 只收到 Codex 事件，beta 只收到 Claude 事件。
    ///   - 负断言：任一邮箱都不得收到第二条串线事件。
    #[test]
    fn hook_tokens_route_same_node_id_to_distinct_workspace_mailboxes() {
        let mut registry = AgentHookRegistry::default();
        let (alpha_token, alpha_receiver, _) = registry.register();
        let (beta_token, beta_receiver, _) = registry.register();
        let alpha_event = parse_hook_event(
            HookSource::Codex,
            7,
            serde_json::json!({"hook_event_name": "Stop"}),
        )
        .expect("alpha hook event 应可构造");
        let beta_event = parse_hook_event(
            HookSource::Claude,
            7,
            serde_json::json!({"hook_event_name": "Stop"}),
        )
        .expect("beta hook event 应可构造");

        let alpha_delivery = registry.deliver(&alpha_token, alpha_event);
        let beta_delivery = registry.deliver(&beta_token, beta_event);
        let alpha_received = alpha_receiver
            .try_recv()
            .expect("alpha 邮箱应收到自己的事件");
        let beta_received = beta_receiver.try_recv().expect("beta 邮箱应收到自己的事件");

        assert_eq!(
            alpha_delivery,
            HookDelivery::Accepted,
            "alpha token 的事件应被明确接受"
        );
        assert_eq!(
            beta_delivery,
            HookDelivery::Accepted,
            "beta token 的事件应被明确接受"
        );
        assert_eq!(
            alpha_received.profile,
            norma::protocol::AgentProfile::Codex,
            "alpha 邮箱只能收到 Codex 事件"
        );
        assert_eq!(
            beta_received.profile,
            norma::protocol::AgentProfile::Claude,
            "beta 邮箱只能收到 Claude 事件"
        );
        assert!(
            alpha_receiver.try_recv().is_err(),
            "alpha 邮箱不得收到 beta 的第二条事件"
        );
        assert!(
            beta_receiver.try_recv().is_err(),
            "beta 邮箱不得收到 alpha 的第二条事件"
        );
    }

    /// Hook 晚到事件：Workspace 注销后必须返回终态失败，不能虚假接受。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   registrations = 1 个 token
    ///   unregisters   = 1 次
    ///   late events   = 1 条 Stop 事件
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 注册邮箱并立即注销 token。
    ///   2. 使用相同 token 投递一条晚到事件。
    ///   3. 检查 receiver 与投递状态。
    ///
    /// 预期结果：
    ///   - 正断言：注销成功且晚到事件返回 Retired（HTTP 层映射 410）。
    ///   - 负断言：receiver 不得收到事件，系统不得返回 Accepted。
    #[test]
    fn retired_hook_token_rejects_late_event_without_false_acceptance() {
        let mut registry = AgentHookRegistry::default();
        let (token, receiver, _) = registry.register();
        let removed = registry.unregister(&token);
        let event = parse_hook_event(
            HookSource::Codex,
            9,
            serde_json::json!({"hook_event_name": "Stop"}),
        )
        .expect("晚到 hook event 应可构造");

        let delivery = registry.deliver(&token, event);

        assert!(removed, "活跃 Hook token 应成功注销");
        assert_eq!(
            delivery,
            HookDelivery::Retired,
            "已注销 token 的晚到事件必须返回 Retired"
        );
        assert_ne!(
            delivery,
            HookDelivery::Accepted,
            "已关闭 Workspace 的晚到事件不得虚假返回 Accepted"
        );
        assert!(
            receiver.try_recv().is_err(),
            "已注销 Workspace 的邮箱不得收到晚到事件"
        );
    }
}
