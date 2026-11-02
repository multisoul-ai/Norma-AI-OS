use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use norma::client::{decode_request_line, encode_response_line};
use norma::codex::{UnixListener, UnixStream};
use norma::protocol::{AgentErrorCode, CardCommandError, WireRequest, WireResponse};
use std::io::{Error, ErrorKind, Read, Write};
#[path = "transport/binding.rs"]
mod binding;
use binding::{prepare_socket_path, SocketBinding, SocketLease};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const STREAM_TIMEOUT: Duration = Duration::from_secs(2);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CLIENT_RECV_INTERVAL: Duration = Duration::from_millis(25);
const CLIENT_QUEUE_CAPACITY: usize = 32;
const REQUEST_QUEUE_CAPACITY: usize = 64;
const CLIENT_WORKER_COUNT: usize = 4;
// Same 64MiB wire budget as the existing Codex relay; queue size alone cannot bound an NDJSON frame.
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;

pub(crate) struct NormaServerRequest {
    pub(crate) request: WireRequest,
    pub(crate) response: Sender<WireResponse>,
}

pub(crate) struct NormaSocketListener {
    receiver: Receiver<NormaServerRequest>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    _binding: SocketBinding,
}

impl NormaSocketListener {
    pub(crate) fn bind(path: PathBuf) -> std::io::Result<Self> {
        let lease = SocketLease::acquire(&path)?;
        prepare_socket_path(&path)?;
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        let binding = SocketBinding::capture(path.clone(), lease)?;
        let (sender, receiver) = bounded(REQUEST_QUEUE_CAPACITY);
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let worker = thread::Builder::new()
            .name("norma-server".to_string())
            .spawn(move || accept_loop(listener, sender, thread_shutdown))?;
        Ok(Self {
            receiver,
            shutdown,
            worker: Some(worker),
            _binding: binding,
        })
    }

    pub(crate) fn try_recv(&self) -> Option<NormaServerRequest> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for NormaSocketListener {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("Norma server thread panicked during shutdown");
            }
        }
    }
}

fn accept_loop(
    listener: UnixListener,
    sender: Sender<NormaServerRequest>,
    shutdown: Arc<AtomicBool>,
) {
    let (stream_sender, stream_receiver) = bounded(CLIENT_QUEUE_CAPACITY);
    let client_workers = spawn_client_workers(stream_receiver, sender, Arc::clone(&shutdown));
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _address)) => match stream_sender.try_send(stream) {
                Ok(()) => {}
                Err(TrySendError::Full(returned_stream)) => {
                    stream = returned_stream;
                    write_overloaded_response(&mut stream, &shutdown);
                }
                Err(TrySendError::Disconnected(_)) => break,
            },
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_POLL_INTERVAL);
            }
            Err(error) => {
                tracing::warn!(%error, "Norma socket accept failed");
                break;
            }
        }
    }
    drop(stream_sender);
    for worker in client_workers {
        if worker.join().is_err() {
            tracing::warn!("Norma client worker panicked during shutdown");
        }
    }
}

fn spawn_client_workers(
    stream_receiver: Receiver<UnixStream>,
    sender: Sender<NormaServerRequest>,
    shutdown: Arc<AtomicBool>,
) -> Vec<JoinHandle<()>> {
    (0..CLIENT_WORKER_COUNT)
        .filter_map(|index| {
            let streams = stream_receiver.clone();
            let requests = sender.clone();
            let worker_shutdown = Arc::clone(&shutdown);
            thread::Builder::new()
                .name(format!("norma-client-{index}"))
                .spawn(move || client_worker_loop(streams, requests, worker_shutdown))
                .map_err(
                    |error| tracing::warn!(%error, index, "failed to spawn Norma client worker"),
                )
                .ok()
        })
        .collect()
}

fn client_worker_loop(
    streams: Receiver<UnixStream>,
    sender: Sender<NormaServerRequest>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) || !streams.is_empty() {
        match streams.recv_timeout(CLIENT_RECV_INTERVAL) {
            Ok(stream) => handle_stream(stream, sender.clone(), &shutdown),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle_stream(
    mut stream: UnixStream,
    sender: Sender<NormaServerRequest>,
    shutdown: &AtomicBool,
) {
    // Accepted sockets may inherit O_NONBLOCK. Only background client workers perform IO.
    if let Err(error) = stream.set_nonblocking(false) {
        tracing::warn!(%error, "could not restore blocking Norma request IO");
        return;
    }
    let response = match read_request_line(&mut stream, shutdown, MAX_REQUEST_BYTES) {
        Ok(line) if line.is_empty() => response_for_error(0, "empty Norma request"),
        Ok(line) => match decode_request_line(&line) {
            Ok(request) => dispatch_request(request, sender, shutdown),
            Err(error) => response_for_error(0, &format!("invalid Norma request: {error}")),
        },
        Err(error) if error.kind() == ErrorKind::Interrupted => {
            response_for_transport_failure(0, TransportFailure::AppShuttingDown)
        }
        Err(error) => response_for_error(0, &format!("could not read Norma request: {error}")),
    };
    write_response(&mut stream, &response, shutdown);
}

fn read_request_line(
    stream: &mut UnixStream,
    shutdown: &AtomicBool,
    limit: usize,
) -> std::io::Result<String> {
    let deadline = Instant::now() + STREAM_TIMEOUT;
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        if shutdown.load(Ordering::Acquire) {
            return Err(ErrorKind::Interrupted.into());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(ErrorKind::TimedOut)?;
        stream.set_read_timeout(Some(remaining.min(CLIENT_RECV_INTERVAL)))?;
        let length = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(length) => length,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(error) => return Err(error),
        };
        let end = buffer[..length]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|i| i + 1);
        let length = end.unwrap_or(length);
        if bytes.len().saturating_add(length) > limit {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Norma request exceeds transport frame limit",
            ));
        }
        bytes.extend_from_slice(&buffer[..length]);
        if end.is_some() {
            break;
        }
    }
    String::from_utf8(bytes).map_err(|error| Error::new(ErrorKind::InvalidData, error))
}

fn dispatch_request(
    request: WireRequest,
    sender: Sender<NormaServerRequest>,
    shutdown: &AtomicBool,
) -> WireResponse {
    let request_id = request.request_id;
    if shutdown.load(Ordering::Acquire) {
        return response_for_transport_failure(request_id, TransportFailure::AppShuttingDown);
    }
    let operation_id = request.operation_id.clone();
    let unknown = || {
        response_for_transport_failure(
            request_id,
            TransportFailure::TimedOut {
                operation_id: operation_id.clone(),
            },
        )
    };
    let (response_sender, response_receiver) = bounded(1);
    match sender.try_send(NormaServerRequest {
        request,
        response: response_sender,
    }) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            return response_for_transport_failure(request_id, TransportFailure::ServerOverloaded)
        }
        Err(TrySendError::Disconnected(_)) => {
            return response_for_transport_failure(request_id, TransportFailure::AppShuttingDown)
        }
    }
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    loop {
        if shutdown.load(Ordering::Acquire) {
            // Once queued, a mutation can have taken effect. Keep its original operation ID.
            return if operation_id.is_some() {
                unknown()
            } else {
                response_for_transport_failure(request_id, TransportFailure::AppShuttingDown)
            };
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return unknown();
        };
        match response_receiver.recv_timeout(remaining.min(CLIENT_RECV_INTERVAL)) {
            Ok(response) => return response,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return unknown(),
        }
    }
}

fn write_overloaded_response(stream: &mut UnixStream, shutdown: &AtomicBool) {
    write_response(
        stream,
        &response_for_transport_failure(0, TransportFailure::ServerOverloaded),
        shutdown,
    );
}
fn write_response(stream: &mut UnixStream, response: &WireResponse, shutdown: &AtomicBool) {
    let Ok(line) = encode_response_line(response) else {
        return;
    };
    let deadline = Instant::now() + STREAM_TIMEOUT;
    let mut remaining = line.as_bytes();
    while !remaining.is_empty() {
        let Some(budget) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        if stream
            .set_write_timeout(Some(budget.min(CLIENT_RECV_INTERVAL)))
            .is_err()
        {
            break;
        }
        match stream.write(remaining) {
            Ok(0) => break,
            Ok(count) => remaining = &remaining[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) && !shutdown.load(Ordering::Acquire) => {}
            Err(_) => break,
        }
        if shutdown.load(Ordering::Acquire) {
            break;
        }
    }
}
fn response_for_error(request_id: u64, message: &str) -> WireResponse {
    WireResponse::new(
        request_id,
        Err(CardCommandError::Runtime {
            message: message.to_string(),
        }),
    )
}

enum TransportFailure {
    ServerOverloaded,
    AppShuttingDown,
    TimedOut { operation_id: Option<String> },
}

fn response_for_transport_failure(request_id: u64, failure: TransportFailure) -> WireResponse {
    let error = match failure {
        TransportFailure::ServerOverloaded => CardCommandError::agent(
            AgentErrorCode::ServerOverloaded,
            "Norma 全局请求入口已达到有界容量，请稍后重试",
            true,
        ),
        TransportFailure::AppShuttingDown => CardCommandError::agent(
            AgentErrorCode::AppShuttingDown,
            "Norma OS 正在退出，未接受新的请求",
            true,
        ),
        TransportFailure::TimedOut {
            operation_id: Some(operation_id),
        } => CardCommandError::outcome_unknown(
            operation_id,
            "请求在收到响应前超时；副作用结果未知，只能复用原 operation_id 查询或重放",
        ),
        TransportFailure::TimedOut { operation_id: None } => CardCommandError::agent(
            AgentErrorCode::AppUnavailable,
            "Norma 请求在收到响应前超时",
            true,
        ),
    };
    WireResponse::new(request_id, Err(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use norma::protocol::AgentErrorCode;

    /// 入口背压与超时：过载、关闭和副作用未知必须使用不同结构化错误。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   transport failures = queue full(1) + shutting down(1) + timed out(1) = 3 类
    ///   operation IDs = timeout 携带固定 ID(1)，其余为 0
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 分别把队列满、App 关闭和带 operation ID 的超时映射为 WireResponse。
    ///   2. 解构三条 Agent failure。
    ///   3. 检查错误码、重试性和副作用未知元数据。
    ///
    /// 预期结果：
    ///   - 正断言：队列满为 server_overloaded，关闭为 app_shutting_down。
    ///   - 正断言：副作用超时为 outcome_unknown，并保留原 operation ID。
    ///   - 负断言：三种入口故障不得退化成无类型 Runtime 错误。
    #[test]
    fn transport_failures_preserve_overload_shutdown_and_unknown_outcome_semantics() {
        let overloaded = response_for_transport_failure(7, TransportFailure::ServerOverloaded);
        let shutting_down = response_for_transport_failure(8, TransportFailure::AppShuttingDown);
        let timed_out = response_for_transport_failure(
            9,
            TransportFailure::TimedOut {
                operation_id: Some("11111111-2222-4333-8444-555555555555".to_string()),
            },
        );

        let Err(CardCommandError::Agent(overloaded)) = overloaded.result else {
            panic!("队列满必须返回结构化 Agent failure");
        };
        let Err(CardCommandError::Agent(shutting_down)) = shutting_down.result else {
            panic!("App 关闭必须返回结构化 Agent failure");
        };
        let Err(CardCommandError::Agent(timed_out)) = timed_out.result else {
            panic!("带 operation ID 的超时必须返回结构化 Agent failure");
        };

        assert_eq!(
            overloaded.code,
            AgentErrorCode::ServerOverloaded,
            "有界队列耗尽必须明确返回 server_overloaded"
        );
        assert_eq!(
            shutting_down.code,
            AgentErrorCode::AppShuttingDown,
            "App receiver 关闭必须明确返回 app_shutting_down"
        );
        assert_eq!(
            timed_out.code,
            AgentErrorCode::OutcomeUnknown,
            "有副作用请求超时必须返回 outcome_unknown"
        );
        assert_eq!(
            timed_out.operation_id.as_deref(),
            Some("11111111-2222-4333-8444-555555555555"),
            "副作用未知错误必须保留首次调用的 operation ID"
        );
        assert!(
            timed_out.outcome_unknown,
            "超时响应必须显式标记副作用结果未知"
        );
        assert!(
            !overloaded.outcome_unknown && !shutting_down.outcome_unknown,
            "尚未入队的过载和已关闭 receiver 不得误报已产生未知副作用"
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/app_transport_deadline_tests.rs"]
mod deadline_tests;
