use crate::TtsError;
use crate::TtsErrorKind;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::header::AUTHORIZATION;
use tungstenite::http::HeaderValue;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const MINIMAX_WEBSOCKET_URL: &str = "wss://api.minimaxi.com/ws/v1/t2a_v2";
const MINIMAX_WEBSOCKET_HOST: &str = "api.minimaxi.com:443";
// 250 ms polling keeps cancellation responsive while avoiding a busy network loop.
const READ_POLL_TIMEOUT: Duration = Duration::from_millis(250);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) trait MiniMaxConnection: Send {
    fn set_setup_deadline(&mut self, _deadline: Instant) -> Result<(), TtsError> {
        Ok(())
    }
    fn finish_setup(&mut self) -> Result<(), TtsError> {
        Ok(())
    }
    fn send_text(&mut self, text: String) -> Result<(), TtsError>;
    fn receive_text(&mut self) -> Result<Option<String>, TtsError>;
    fn close(&mut self);
}

pub(crate) trait MiniMaxTransport: Send + Sync {
    fn connect(
        &self,
        api_key: &str,
        deadline: Instant,
    ) -> Result<Box<dyn MiniMaxConnection>, TtsError>;
}

pub(crate) struct ProductionMiniMaxTransport;

impl MiniMaxTransport for ProductionMiniMaxTransport {
    fn connect(
        &self,
        api_key: &str,
        deadline: Instant,
    ) -> Result<Box<dyn MiniMaxConnection>, TtsError> {
        let mut request = MINIMAX_WEBSOCKET_URL.into_client_request().map_err(|_| {
            TtsError::new(
                TtsErrorKind::Configuration,
                "could not build MiniMax WebSocket request",
            )
        })?;
        let authorization = HeaderValue::from_str(&format!("Bearer {api_key}")).map_err(|_| {
            TtsError::new(
                TtsErrorKind::Authentication,
                "MiniMax credential contains invalid header characters",
            )
        })?;
        request.headers_mut().insert(AUTHORIZATION, authorization);
        let socket = connect_tcp(deadline)?;
        let setup_timeout = remaining(deadline)?;
        socket
            .set_read_timeout(Some(setup_timeout))
            .and_then(|()| socket.set_write_timeout(Some(setup_timeout)))
            .map_err(|_| setup_transport_error("could not configure MiniMax setup timeout"))?;
        let (mut socket, _) = tungstenite::client_tls_with_config(request, socket, None, None)
            .map_err(|error| match error {
                tungstenite::HandshakeError::Failure(error) => connect_error(error),
                tungstenite::HandshakeError::Interrupted(_) => {
                    setup_transport_error("MiniMax WebSocket setup was interrupted")
                }
            })?;
        configure_socket_timeouts(socket.get_mut())?;
        Ok(Box::new(ProductionMiniMaxConnection { socket }))
    }
}

fn connect_tcp(deadline: Instant) -> Result<TcpStream, TtsError> {
    let addresses = resolve_addresses(deadline)?;
    for address in addresses {
        match TcpStream::connect_timeout(&address, remaining(deadline)?) {
            Ok(socket) => return Ok(socket),
            Err(_) => continue,
        }
    }
    Err(setup_transport_error(
        "MiniMax WebSocket connection timed out",
    ))
}

fn resolve_addresses(deadline: Instant) -> Result<Vec<std::net::SocketAddr>, TtsError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("minimax_tts_dns".to_string())
        .spawn(move || {
            let resolved = MINIMAX_WEBSOCKET_HOST
                .to_socket_addrs()
                .map(|addresses| addresses.collect::<Vec<_>>());
            let _ = sender.send(resolved);
        })
        .map_err(|_| setup_transport_error("could not start MiniMax DNS resolution"))?;
    receiver
        .recv_timeout(remaining(deadline)?)
        .map_err(|_| setup_transport_error("MiniMax DNS resolution timed out"))?
        .map_err(|_| setup_transport_error("could not resolve MiniMax WebSocket host"))
}

fn remaining(deadline: Instant) -> Result<Duration, TtsError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| setup_transport_error("MiniMax WebSocket setup timed out"))
}

struct ProductionMiniMaxConnection {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl MiniMaxConnection for ProductionMiniMaxConnection {
    fn set_setup_deadline(&mut self, deadline: Instant) -> Result<(), TtsError> {
        configure_socket_write_timeout(self.socket.get_mut(), remaining(deadline)?)
    }

    fn finish_setup(&mut self) -> Result<(), TtsError> {
        configure_socket_timeouts(self.socket.get_mut())
    }

    fn send_text(&mut self, text: String) -> Result<(), TtsError> {
        self.socket
            .send(Message::Text(text.into()))
            .map_err(transport_error)
    }

    fn receive_text(&mut self) -> Result<Option<String>, TtsError> {
        match self.socket.read() {
            Ok(Message::Text(text)) => Ok(Some(text.to_string())),
            Ok(Message::Ping(_) | Message::Pong(_)) => Ok(None),
            Ok(Message::Close(_)) => Err(TtsError::new(
                TtsErrorKind::Transport,
                "MiniMax closed the WebSocket before the task completed",
            )),
            Ok(Message::Binary(_) | Message::Frame(_)) => Err(TtsError::new(
                TtsErrorKind::Protocol,
                "MiniMax returned a non-text protocol frame",
            )),
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
            {
                Ok(None)
            }
            Err(error) => Err(transport_error(error)),
        }
    }

    fn close(&mut self) {
        let _ = self.socket.close(None);
    }
}

fn configure_socket_timeouts(stream: &mut MaybeTlsStream<TcpStream>) -> Result<(), TtsError> {
    let socket = match stream {
        MaybeTlsStream::Plain(socket) => socket,
        MaybeTlsStream::Rustls(stream) => &mut stream.sock,
        _ => {
            return Err(TtsError::new(
                TtsErrorKind::Transport,
                "unsupported MiniMax TLS stream",
            ))
        }
    };
    socket
        .set_read_timeout(Some(READ_POLL_TIMEOUT))
        .and_then(|()| socket.set_write_timeout(Some(WRITE_TIMEOUT)))
        .map_err(|_| {
            TtsError::new(
                TtsErrorKind::Transport,
                "could not configure MiniMax socket timeouts",
            )
        })
}

fn configure_socket_write_timeout(
    stream: &mut MaybeTlsStream<TcpStream>,
    timeout: Duration,
) -> Result<(), TtsError> {
    let socket = match stream {
        MaybeTlsStream::Plain(socket) => socket,
        MaybeTlsStream::Rustls(stream) => &mut stream.sock,
        _ => {
            return Err(TtsError::new(
                TtsErrorKind::Transport,
                "unsupported MiniMax TLS stream",
            ))
        }
    };
    socket
        .set_write_timeout(Some(timeout))
        .map_err(|_| setup_transport_error("could not configure MiniMax setup write deadline"))
}

fn connect_error(error: tungstenite::Error) -> TtsError {
    if let tungstenite::Error::Http(response) = &error {
        if matches!(response.status().as_u16(), 401 | 403) {
            return TtsError::new(
                TtsErrorKind::Authentication,
                "MiniMax rejected the supplied credential",
            );
        }
    }
    transport_error(error).retryable_before_acceptance()
}

fn transport_error(_error: tungstenite::Error) -> TtsError {
    TtsError::new(
        TtsErrorKind::Transport,
        "MiniMax WebSocket transport failed",
    )
}

fn setup_transport_error(message: &'static str) -> TtsError {
    TtsError::new(TtsErrorKind::Transport, message).retryable_before_acceptance()
}
