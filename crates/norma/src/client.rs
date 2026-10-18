use crate::orchestration::{load_provider_authorization, InstallPaths, OrchestrationProvider};
use crate::protocol::{
    AgentErrorCode, CardCommand, CardCommandError, CardCommandResult, ProviderAuthorization,
    WireRequest, WireResponse, CURRENT_PROTOCOL_VERSION,
};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SOCKET_ENV: &str = "NORMA_SOCKET";
#[cfg(not(debug_assertions))]
const DEFAULT_SOCKET_NAME: &str = "multisoul-norma.sock";
#[cfg(debug_assertions)]
const DEFAULT_SOCKET_NAME: &str = "multisoul-norma-dev.sock";
const WORKSPACE_ID_ENV: &str = "NORMA_WORKSPACE_ID";
const WORKSPACE_ROUTE_ID_ENV: &str = "NORMA_WORKSPACE_ROUTE_ID";

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
    source_workspace_id: Option<String>,
    source_workspace_route_id: Option<String>,
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
            source_workspace_id: None,
            source_workspace_route_id: None,
            source_node_id: Some(source_node_id),
        })
    }

    pub fn from_values_with_workspace(
        provider: Option<&str>,
        source_node_id: Option<&str>,
        source_workspace_id: Option<&str>,
        source_workspace_route_id: Option<&str>,
        paths: &InstallPaths,
        orchestration_command: bool,
    ) -> Result<Self, ClientError> {
        let mut context =
            Self::from_values(provider, source_node_id, paths, orchestration_command)?;
        if !orchestration_command {
            return Ok(context);
        }
        let workspace_id = non_blank_context_value(source_workspace_id).ok_or_else(|| {
            ClientError::Command(CardCommandError::agent(
                AgentErrorCode::ClientUpgradeRequired,
                "当前 Agent Session 缺少 Workspace 身份；请重新激活该 Agent",
                false,
            ))
        })?;
        let route_id = non_blank_context_value(source_workspace_route_id).ok_or_else(|| {
            ClientError::Command(CardCommandError::agent(
                AgentErrorCode::ClientUpgradeRequired,
                "当前 Agent Session 缺少 Workspace route；请重新激活该 Agent",
                false,
            ))
        })?;
        context.source_workspace_id = Some(workspace_id.to_string());
        context.source_workspace_route_id = Some(route_id.to_string());
        Ok(context)
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
        let source_workspace_id = env::var(WORKSPACE_ID_ENV).ok();
        let source_workspace_route_id = env::var(WORKSPACE_ROUTE_ID_ENV).ok();
        Self::from_values_with_workspace(
            provider.as_deref(),
            source_node_id.as_deref(),
            source_workspace_id.as_deref(),
            source_workspace_route_id.as_deref(),
            &paths,
            true,
        )
    }
}

fn non_blank_context_value(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
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
    send_command_with_request_metadata(command, None, None)
}

pub fn send_command_with_request_metadata(
    command: CardCommand,
    target_workspace_id: Option<String>,
    operation_id: Option<String>,
) -> Result<ClientResponse, ClientError> {
    let context = ClientRequestContext::from_current_process(&command)?;
    send_command_to_path_with_context_and_metadata(
        default_socket_path(),
        command,
        context,
        target_workspace_id,
        operation_id,
    )
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
    send_command_to_path_with_context_and_metadata(socket_path, command, context, None, None)
}

pub fn send_command_to_path_with_context_and_metadata(
    socket_path: impl AsRef<Path>,
    command: CardCommand,
    context: ClientRequestContext,
    target_workspace_id: Option<String>,
    operation_id: Option<String>,
) -> Result<ClientResponse, ClientError> {
    let request_id = request_id();
    let mut request = build_wire_request(request_id, command, context);
    request.target_workspace_id = target_workspace_id;
    request.operation_id = operation_id;
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
    if response.protocol_version != CURRENT_PROTOCOL_VERSION {
        return Err(ClientError::Command(CardCommandError::agent(
            AgentErrorCode::ProtocolVersionMismatch,
            format!(
                "Norma App protocol {} 与 CLI protocol {} 不兼容",
                response.protocol_version, CURRENT_PROTOCOL_VERSION
            ),
            false,
        )));
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
        protocol_version: CURRENT_PROTOCOL_VERSION,
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        request_id,
        operation_id: None,
        authorization: context.authorization,
        source_workspace_id: context.source_workspace_id,
        source_workspace_route_id: context.source_workspace_route_id,
        target_workspace_id: None,
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
        CardCommand::ListWorkspaces
            | CardCommand::ListAgents
            | CardCommand::DispatchAgentTasks { .. }
            | CardCommand::AgentTaskStatus { .. }
            | CardCommand::AgentTaskResult { .. }
            | CardCommand::AgentTaskCancel { .. }
            | CardCommand::Present { .. }
            | CardCommand::Ego { .. }
    )
}

fn parse_provider(value: &str) -> Option<OrchestrationProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "claude" => Some(OrchestrationProvider::Claude),
        "codex" => Some(OrchestrationProvider::Codex),
        "cursor" => Some(OrchestrationProvider::Cursor),
        "opencode" => Some(OrchestrationProvider::OpenCode),
        "grok" => Some(OrchestrationProvider::Grok),
        "kimi" => Some(OrchestrationProvider::Kimi),
        "zcode" => Some(OrchestrationProvider::ZCode),
        "pi" => Some(OrchestrationProvider::Pi),
        "hermes" => Some(OrchestrationProvider::Hermes),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_SOCKET_NAME;

    /// Debug 构建的 Norma App/CLI 必须使用独立 socket，避免误连正在运行的正式版。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   build channel = debug(1)
    ///   expected socket = multisoul-norma-dev.sock(1)
    ///   forbidden production socket = multisoul-norma.sock(1)
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在 debug 测试构建中读取内置默认 socket 文件名。
    ///   2. 与开发版专用文件名比较。
    ///   3. 与正式版共享文件名做负向比较。
    ///
    /// 预期结果：
    ///   - 正断言：debug App 与 debug CLI 默认使用 `multisoul-norma-dev.sock`。
    ///   - 负断言：debug 构建不得继续占用正式版的 `multisoul-norma.sock`。
    #[cfg(debug_assertions)]
    #[test]
    fn debug_build_uses_an_isolated_norma_socket_name() {
        assert_eq!(
            DEFAULT_SOCKET_NAME, "multisoul-norma-dev.sock",
            "debug Norma App/CLI 必须使用开发版专用 socket，避免路由到正式版进程"
        );
        assert_ne!(
            DEFAULT_SOCKET_NAME, "multisoul-norma.sock",
            "debug 构建不得与正式版争用同一个 Norma socket"
        );
    }

    /// Release 构建必须保留稳定的生产 socket 名称，不继承开发版隔离命名。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   build channel = release(1)
    ///   expected socket = multisoul-norma.sock(1)
    ///   forbidden debug socket = multisoul-norma-dev.sock(1)
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在 release 测试构建中读取内置默认 socket 文件名。
    ///   2. 与生产稳定文件名比较。
    ///   3. 与开发版文件名做负向比较。
    ///
    /// 预期结果：
    ///   - 正断言：release App 与 CLI 继续使用 `multisoul-norma.sock`。
    ///   - 负断言：release 构建不得连接开发版专用 socket。
    #[cfg(not(debug_assertions))]
    #[test]
    fn release_build_keeps_the_production_norma_socket_name() {
        assert_eq!(
            DEFAULT_SOCKET_NAME, "multisoul-norma.sock",
            "release Norma App/CLI 必须保留稳定生产 socket 名称"
        );
        assert_ne!(
            DEFAULT_SOCKET_NAME, "multisoul-norma-dev.sock",
            "release 构建不得连接开发版专用 socket"
        );
    }
}
