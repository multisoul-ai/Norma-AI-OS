//! 单次回环监听：只绑定 127.0.0.1 随机端口，收到合法 `/callback` 请求、超时或取消即结束。
//!
//! 阻塞实现，必须在后台线程调用。

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(40);
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const DONE_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>Norma</title>\
<body style=\"font-family:-apple-system,sans-serif;text-align:center;padding-top:20vh\">\
<h2>可以回到 Norma 了 · You can return to Norma</h2></body>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoopbackError {
    Timeout,
    Cancelled,
    Io(String),
}

pub(crate) struct LoopbackListener {
    listener: TcpListener,
    port: u16,
}

impl LoopbackListener {
    pub(crate) fn bind() -> Result<Self, LoopbackError> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(io_error)?;
        listener.set_nonblocking(true).map_err(io_error)?;
        let port = listener.local_addr().map_err(io_error)?.port();
        Ok(Self { listener, port })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// 返回请求目标（如 `/callback?code=…&state=…`）。其它路径与非本机 Host 的请求被忽略并继续等待。
    pub(crate) fn wait_for_callback(
        self,
        timeout: Duration,
        cancel: &Arc<AtomicBool>,
    ) -> Result<String, LoopbackError> {
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(LoopbackError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(LoopbackError::Timeout);
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(target) = self.read_callback(stream) {
                        return Ok(target);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(error) => return Err(io_error(error)),
            }
        }
    }

    fn read_callback(&self, stream: TcpStream) -> Option<String> {
        stream.set_nonblocking(false).ok()?;
        stream.set_read_timeout(Some(READ_TIMEOUT)).ok()?;
        let mut reader = BufReader::new(stream.try_clone().ok()?);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).ok()?;
        let mut parts = request_line.split_whitespace();
        let (method, target) = (parts.next()?, parts.next()?);
        let mut host_ok = false;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).ok()? == 0 || line.trim().is_empty() {
                break;
            }
            if let Some(host) = line.to_ascii_lowercase().strip_prefix("host:") {
                host_ok = host.trim() == format!("127.0.0.1:{}", self.port);
            }
        }
        if method != "GET" || !host_ok || !target.starts_with("/callback") {
            respond(stream, "404 Not Found", "");
            return None;
        }
        respond(stream, "200 OK", DONE_PAGE);
        Some(target.to_string())
    }
}

fn respond(mut stream: TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn io_error(error: std::io::Error) -> LoopbackError {
    LoopbackError::Io(error.to_string())
}
