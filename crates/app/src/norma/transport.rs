use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use norma::client::{decode_request_line, encode_response_line};
use norma::protocol::{AgentErrorCode, CardCommandError, WireRequest, WireResponse};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Error, ErrorKind, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const STREAM_TIMEOUT: Duration = Duration::from_secs(2);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CLIENT_RECV_INTERVAL: Duration = Duration::from_millis(25);
const CLIENT_QUEUE_CAPACITY: usize = 32;
const REQUEST_QUEUE_CAPACITY: usize = 64;
const CLIENT_WORKER_COUNT: usize = 4;

pub(crate) struct NormaServerRequest {
    pub(crate) request: WireRequest,
    pub(crate) response: Sender<WireResponse>,
}

struct SocketLease {
    _file: File,
}

impl SocketLease {
    fn acquire(socket_path: &Path) -> std::io::Result<Self> {
        let lock_path = socket_path.with_extension("lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(lock_path)?;
        let lock_result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if lock_result != 0 {
            let error = Error::last_os_error();
            if matches!(error.kind(), ErrorKind::WouldBlock) {
                return Err(Error::new(
                    ErrorKind::AddrInUse,
                    "another Norma endpoint owns the socket lease",
                ));
            }
            return Err(error);
        }
        Ok(Self { _file: file })
    }
}

struct SocketBinding {
    path: PathBuf,
    device: u64,
    inode: u64,
    _lease: SocketLease,
}

impl SocketBinding {
    fn capture(path: PathBuf, lease: SocketLease) -> std::io::Result<Self> {
        let metadata = fs::symlink_metadata(&path)?;
        Ok(Self {
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
            _lease: lease,
        })
    }

    fn still_owns_path(&self) -> bool {
        fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.file_type().is_socket()
                && metadata.dev() == self.device
                && metadata.ino() == self.inode
        })
    }
}

impl Drop for SocketBinding {
    fn drop(&mut self) {
        if !self.still_owns_path() {
            tracing::debug!(
                path = %self.path.display(),
                "Norma socket binding was replaced; skipped stale cleanup"
            );
            return;
        }
        if let Err(error) = fs::remove_file(&self.path) {
            tracing::warn!(
                path = %self.path.display(),
                %error,
                "failed to remove owned Norma socket"
            );
        }
    }
}

pub(crate) struct NormaSocketListener {
    receiver: Receiver<NormaServerRequest>,
    shutdown: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    binding: SocketBinding,
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
            binding,
        })
    }

    pub(crate) fn try_recv(&self) -> Option<NormaServerRequest> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for NormaSocketListener {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = UnixStream::connect(&self.binding.path);
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("Norma server thread panicked during shutdown");
            }
        }
    }
}

fn prepare_socket_path(path: &Path) -> std::io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_socket() {
        return Err(Error::new(
            ErrorKind::AddrInUse,
            "Norma socket path is occupied by a non-socket file",
        ));
    }
    if UnixStream::connect(path).is_ok() {
        return Err(Error::new(
            ErrorKind::AddrInUse,
            "an active Norma endpoint already owns the socket",
        ));
    }
    fs::remove_file(path)
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
                    write_overloaded_response(&mut stream);
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
            Ok(stream) => handle_stream(stream, sender.clone()),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle_stream(mut stream: UnixStream, sender: Sender<NormaServerRequest>) {
    let _ = stream.set_read_timeout(Some(STREAM_TIMEOUT));
    let _ = stream.set_write_timeout(Some(STREAM_TIMEOUT));
    let mut line = String::new();
    let read_result = BufReader::new(&stream).read_line(&mut line);
    let response = match read_result {
        Ok(0) => response_for_error(0, "empty Norma request"),
        Ok(_) => match decode_request_line(&line) {
            Ok(request) => dispatch_request(request, sender),
            Err(error) => response_for_error(0, &format!("invalid Norma request: {error}")),
        },
        Err(error) => response_for_error(0, &format!("could not read Norma request: {error}")),
    };
    write_response(&mut stream, &response);
}

fn dispatch_request(request: WireRequest, sender: Sender<NormaServerRequest>) -> WireResponse {
    let request_id = request.request_id;
    let operation_id = request.operation_id.clone();
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
    response_receiver
        .recv_timeout(RESPONSE_TIMEOUT)
        .unwrap_or_else(|_| {
            response_for_transport_failure(request_id, TransportFailure::TimedOut { operation_id })
        })
}

fn write_overloaded_response(stream: &mut UnixStream) {
    write_response(
        stream,
        &response_for_transport_failure(0, TransportFailure::ServerOverloaded),
    );
}

fn write_response(stream: &mut UnixStream, response: &WireResponse) {
    if let Ok(line) = encode_response_line(response) {
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.flush();
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
