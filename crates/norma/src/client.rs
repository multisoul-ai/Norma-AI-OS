use crate::orchestration::{load_provider_authorization, InstallPaths, OrchestrationProvider};
use crate::protocol::{
    AgentErrorCode, CardCommand, CardCommandError, CardCommandResult, ProviderAuthorization,
    WireRequest, WireResponse,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClientRequestContext {
    authorization: Option<ProviderAuthorization>,
    source_node_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientResponse {
    pub request_id: u64,
    pub result: CardCommandResult,
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

impl ClientError {
    pub fn agent_error_code(&self) -> Option<AgentErrorCode> {
        match self {
            Self::Command(CardCommandError::Agent(failure)) => Some(failure.code),
            _ => None,
        }
    }

    pub fn into_card_error(self) -> CardCommandError {
        match self {
            Self::Command(error) => error,
            Self::Io(error) => CardCommandError::agent(
                AgentErrorCode::AppUnavailable,
                format!("Norma OS 本地服务不可用：{error}"),
                true,
            ),
            other => {
                CardCommandError::agent(AgentErrorCode::InternalError, other.to_string(), false)
            }
        }
    }
}

impl ClientRequestContext {
    pub fn from_values(
        provider: Option<&str>,
        source_node_id: Option<&str>,
        paths: &InstallPaths,
        orchestration_command: bool,
    ) -> Result<Self, ClientError> {
        if !orchestration_command {
            return Ok(Self::default());
        }
        let provider = provider.and_then(parse_provider).ok_or_else(|| {
            ClientError::Command(CardCommandError::agent(
                AgentErrorCode::SourceAgentUnavailable,
                "当前命令缺少有效的 Agent Provider 上下文",
                false,
            ))
        })?;
        let source_node_id = source_node_id
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|node_id| *node_id > 0)
            .ok_or_else(|| {
                ClientError::Command(CardCommandError::agent(
                    AgentErrorCode::SourceAgentUnavailable,
                    "当前命令缺少有效的来源 Agent 节点",
                    false,
                ))
            })?;
        let authorization = load_provider_authorization(paths, provider)
            .map_err(|error| {
                ClientError::Command(CardCommandError::agent(
                    AgentErrorCode::ProviderNotInstalled,
                    format!("{} 编排凭据不可用：{error}", provider.display_name()),
                    false,
                ))
            })?
            .ok_or_else(|| {
                ClientError::Command(CardCommandError::agent(
                    AgentErrorCode::ProviderNotInstalled,
                    format!("{} 尚未安装编排能力", provider.display_name()),
                    false,
                ))
            })?;
        Ok(Self {
            authorization: Some(authorization),
            source_node_id: Some(source_node_id),
        })
    }

    fn from_current_process(command: &CardCommand) -> Result<Self, ClientError> {
        let orchestration_command = is_orchestration_command(command);
        if !orchestration_command {
            return Ok(Self::default());
        }
        let paths = InstallPaths::for_current_process().map_err(|error| {
            ClientError::Command(CardCommandError::agent(
                AgentErrorCode::ProviderNotInstalled,
                format!("无法定位 Norma OS 编排安装：{error}"),
                false,
            ))
        })?;
        let provider = env::var("MULTISOUL_AGENT_PROFILE").ok();
        let source_node_id = env::var("MULTISOUL_NODE_ID").ok();
        Self::from_values(provider.as_deref(), source_node_id.as_deref(), &paths, true)
    }
}

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
    send_command_with_metadata(command).map(|response| response.result)
}

pub fn send_command_with_metadata(command: CardCommand) -> Result<ClientResponse, ClientError> {
    let context = ClientRequestContext::from_current_process(&command)?;
    send_command_to_path_with_context(default_socket_path(), command, context)
}

pub fn send_command_to_path(
    socket_path: impl AsRef<Path>,
    command: CardCommand,
) -> Result<CardCommandResult, ClientError> {
    send_command_to_path_with_context(socket_path, command, ClientRequestContext::default())
        .map(|response| response.result)
}

pub fn send_command_to_path_with_context(
    socket_path: impl AsRef<Path>,
    command: CardCommand,
    context: ClientRequestContext,
) -> Result<ClientResponse, ClientError> {
    let request_id = request_id();
    let request = build_wire_request(request_id, command, context);
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

    response
        .result
        .map(|result| ClientResponse { request_id, result })
        .map_err(ClientError::Command)
}

pub fn build_wire_request(
    request_id: u64,
    command: CardCommand,
    context: ClientRequestContext,
) -> WireRequest {
    WireRequest {
        request_id,
        authorization: context.authorization,
        source_node_id: context.source_node_id,
        command,
    }
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

fn is_orchestration_command(command: &CardCommand) -> bool {
    matches!(
        command,
        CardCommand::ListAgents
            | CardCommand::DispatchAgentTasks { .. }
            | CardCommand::AgentTaskStatus { .. }
            | CardCommand::AgentTaskCancel { .. }
    )
}

fn parse_provider(value: &str) -> Option<OrchestrationProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "claude" => Some(OrchestrationProvider::Claude),
        "codex" => Some(OrchestrationProvider::Codex),
        "cursor" => Some(OrchestrationProvider::Cursor),
        "opencode" => Some(OrchestrationProvider::OpenCode),
        "grok" => Some(OrchestrationProvider::Grok),
        _ => None,
    }
}
