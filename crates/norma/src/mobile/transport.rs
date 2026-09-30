use anyhow::Result;
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Instant,
};
use tungstenite::{
    handshake::server::{Request, Response},
    protocol::WebSocketConfig,
    WebSocket,
};

pub type MobileSocket = WebSocket<StreamOwned<ServerConnection, MobileTcp>>;

/// Per-operation absolute deadline; dripping one byte at a time cannot extend a handshake.
pub struct MobileTcp {
    socket: TcpStream,
    deadline: Instant,
}

impl MobileTcp {
    /// The serialized background owner sets a new deadline before each request/response.
    pub fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = deadline;
    }

    fn remaining(&self) -> io::Result<std::time::Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "mobile operation timed out"))
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for MobileTcp {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.socket.as_fd()
    }
}

impl Read for MobileTcp {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(buf)
    }
}

impl Write for MobileTcp {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

/// TLS is mandatory. This only validates transport, NOT device authorization. The caller must
/// bound active sockets, retain shutdown handles and authenticate before exposing any project.
pub fn accept_mobile_websocket(
    socket: TcpStream,
    config: Arc<ServerConfig>,
    deadline: Instant,
) -> Result<MobileSocket> {
    // macOS accepted sockets inherit O_NONBLOCK; this worker uses deadline-bounded blocking IO.
    socket.set_nonblocking(false)?;
    // Small interactive updates must not wait for Nagle/delayed-ACK batching.
    socket.set_nodelay(true)?;
    let stream = MobileTcp { socket, deadline };
    stream.remaining()?;
    let tls = StreamOwned::new(ServerConnection::new(config)?, stream);
    // Requests (not history responses) are capped at 256 KiB, supporting ~85k Chinese characters.
    // Each frame is capped too, and outgoing buffering cannot grow without bound.
    let config = WebSocketConfig::default()
        .max_message_size(Some(256 * 1024))
        .max_frame_size(Some(256 * 1024))
        .write_buffer_size(0)
        .max_write_buffer_size(512 * 1024);
    let callback = |request: &Request, response: Response| {
        if request.uri().path() != "/mobile"
            || request.uri().query().is_some()
            || request.headers().contains_key("origin")
        {
            let mut error = tungstenite::handshake::server::ErrorResponse::new(Some(
                "mobile endpoint unavailable".into(),
            ));
            *error.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
            return Err(error);
        }
        Ok(response)
    };
    tungstenite::accept_hdr_with_config(tls, callback, Some(config))
        .map_err(|_| anyhow::anyhow!("mobile TLS/WebSocket handshake failed"))
}
