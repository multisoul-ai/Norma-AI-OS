use crate::protocol::{
    CardCommand, CardCommandError, CardCommandResult, WireRequest, WireResponse,
};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SOCKET_ENV: &str = "NORMA_SOCKET";
const DEFAULT_SOCKET_NAME: &str = "multisoul-norma.sock";

#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Command(CardCommandError),
    EmptyResponse,
    MismatchedResponseId { expected: u64, actual: u64 },
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Command(error) => write!(f, "{error}"),
            Self::EmptyResponse => write!(f, "Norma server returned an empty response"),
            Self::MismatchedResponseId { expected, actual } => write!(
                f,
                "Norma response id mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<std::io::Error> for ClientError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn default_socket_path() -> PathBuf {
    env::var_os(SOCKET_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join(DEFAULT_SOCKET_NAME))
}

pub fn send_command(command: CardCommand) -> Result<CardCommandResult, ClientError> {
    send_command_to_path(default_socket_path(), command)
}

pub fn send_command_to_path(
    socket_path: impl AsRef<Path>,
    command: CardCommand,
) -> Result<CardCommandResult, ClientError> {
    let request_id = request_id();
    let request = WireRequest {
        request_id,
        command,
    };
    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(encode_request_line(&request)?.as_bytes())?;
    stream.flush()?;

    let mut response_line = String::new();
    BufReader::new(stream).read_line(&mut response_line)?;
    if response_line.is_empty() {
        return Err(ClientError::EmptyResponse);
    }

    let response = decode_response_line(&response_line)?;
    if response.request_id != request_id {
        return Err(ClientError::MismatchedResponseId {
            expected: request_id,
            actual: response.request_id,
        });
    }

    response.result.map_err(ClientError::Command)
}

pub fn encode_request_line(request: &WireRequest) -> Result<String, serde_json::Error> {
    encode_line(request)
}

pub fn decode_request_line(line: &str) -> Result<WireRequest, serde_json::Error> {
    serde_json::from_str(line.trim_end_matches('\n'))
}

pub fn encode_response_line(response: &WireResponse) -> Result<String, serde_json::Error> {
    encode_line(response)
}

pub fn decode_response_line(line: &str) -> Result<WireResponse, serde_json::Error> {
    serde_json::from_str(line.trim_end_matches('\n'))
}

fn encode_line<T: serde::Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    Ok(line)
}

fn request_id() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(1)
}
