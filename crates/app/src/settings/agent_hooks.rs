use canvas::AgentProfile as CanvasAgentProfile;
#[cfg(test)]
use canvas::CanvasNodeKind;
#[cfg(feature = "e2e-test-support")]
use crossbeam_channel::Sender;
use crossbeam_channel::{unbounded, Receiver};
use norma::agent_hook::{
    extract_prompt_text, task_state_for_provider_event, AgentHookEvent, AgentHookTaskState,
};
use runtime::{AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, Runtime};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub(crate) struct AgentHookServer {
    receiver: Receiver<AgentHookEvent>,
    #[cfg(feature = "e2e-test-support")]
    event_sender: Sender<AgentHookEvent>,
    endpoint: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct AgentHookDrainReport {
    pub(crate) changed: bool,
    pub(crate) ready_node_ids: Vec<u64>,
    pub(crate) terminal_node_ids: Vec<u64>,
    pub(crate) events: Vec<AgentHookEvent>,
}

impl AgentHookServer {
    pub(crate) fn start() -> Self {
        let (sender, receiver) = unbounded();
        #[cfg(feature = "e2e-test-support")]
        let event_sender = sender.clone();
        let token = random_token();
        let listener = match TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!(%error, "failed to bind agent hook server");
                return Self {
                    receiver,
                    #[cfg(feature = "e2e-test-support")]
                    event_sender,
                    endpoint: None,
                    token: None,
                };
            }
        };
        let port = listener.local_addr().ok().map(|addr| addr.port());
        let Some(port) = port else {
            return Self {
                receiver,
                #[cfg(feature = "e2e-test-support")]
                event_sender,
                endpoint: None,
                token: None,
            };
        };
        let thread_token = token.clone();
        if let Err(error) = thread::Builder::new()
            .name("agent-hook-server".to_string())
            .spawn(move || accept_loop(listener, thread_token, sender))
        {
            tracing::warn!(%error, "failed to spawn agent hook server thread");
            return Self {
                receiver,
                #[cfg(feature = "e2e-test-support")]
                event_sender,
                endpoint: None,
                token: None,
            };
        }
        Self {
            receiver,
            #[cfg(feature = "e2e-test-support")]
            event_sender,
            endpoint: Some(format!("http://127.0.0.1:{port}/agent-hooks")),
            token: Some(token),
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.receiver.is_empty()
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn enqueue_event_for_e2e(&self, event: AgentHookEvent) -> Result<(), String> {
        self.event_sender
            .send(event)
            .map_err(|_| "Agent Hook e2e event queue is closed".to_string())
    }

    pub(crate) fn runtime_config(&self) -> Option<AgentHookRuntimeConfig> {
        Some(AgentHookRuntimeConfig {
            endpoint: self.endpoint.clone()?,
            token: self.token.clone()?,
        })
    }

    pub(crate) fn disable_for_test(&mut self) {
        self.endpoint = None;
        self.token = None;
    }

    pub(crate) fn drain_into_runtime(&self, runtime: &mut Runtime) -> AgentHookDrainReport {
        let mut report = AgentHookDrainReport::default();
        while let Ok(event) = self.receiver.try_recv() {
            let node_id = event.node_id;
            let is_ready = matches!(
                agent_task_state_from_hook_event(&event),
                Some(AgentTaskState::Ready)
            );
            let is_terminal = matches!(
                agent_task_state_from_hook_event(&event),
                Some(AgentTaskState::Done | AgentTaskState::Failed { .. } | AgentTaskState::Ended)
            );
            report.events.push(event.clone());
            if apply_agent_hook_event(runtime, event) {
                report.changed = true;
                if is_ready {
                    report.ready_node_ids.push(node_id);
                }
                if is_terminal {
                    report.terminal_node_ids.push(node_id);
                }
            }
        }
        report
    }
}

pub(crate) fn apply_agent_hook_event(runtime: &mut Runtime, event: AgentHookEvent) -> bool {
    let Some(to) = agent_task_state_from_hook_event(&event) else {
        tracing::debug!(provider_event = %event.provider_event, "ignored unmapped agent hook event");
        return false;
    };
    let task_title = if event.provider_event == "UserPromptSubmit" {
        extract_prompt_text(&event.raw_json)
    } else {
        None
    };
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: event.node_id,
        profile: canvas_profile(event.profile),
        provider_event: event.provider_event,
        to,
        task_title,
        session_id: event.session_id,
    })
}

fn accept_loop(
    listener: TcpListener,
    token: String,
    sender: crossbeam_channel::Sender<AgentHookEvent>,
) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let token = token.clone();
                let sender = sender.clone();
                let _ = thread::Builder::new()
                    .name("agent-hook-client".to_string())
                    .spawn(move || handle_stream(stream, &token, sender));
            }
            Err(error) => tracing::warn!(%error, "agent hook accept failed"),
        }
    }
}

fn handle_stream(
    mut stream: TcpStream,
    token: &str,
    sender: crossbeam_channel::Sender<AgentHookEvent>,
) {
    let response = match read_authorized_event(&stream, token) {
        Ok(event) => {
            let _ = sender.send(event);
            "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n"
        }
        Err(AgentHookHttpError::Unauthorized) => {
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n"
        }
        Err(_) => "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n",
    };
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn read_authorized_event(
    stream: &TcpStream,
    token: &str,
) -> Result<AgentHookEvent, AgentHookHttpError> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    if !request_line.starts_with("POST /agent-hooks ") {
        return Err(AgentHookHttpError::BadRequest);
    }
    let mut authorized = false;
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Authorization: Bearer ") {
            authorized = value == token;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
            content_len = value.parse::<usize>().unwrap_or(0);
        }
    }
    if !authorized {
        return Err(AgentHookHttpError::Unauthorized);
    }
    let mut body = vec![0u8; content_len];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|_| AgentHookHttpError::Json)
}

fn agent_task_state_from_hook_event(event: &AgentHookEvent) -> Option<AgentTaskState> {
    match task_state_for_provider_event(event)? {
        AgentHookTaskState::Ready => Some(AgentTaskState::Ready),
        AgentHookTaskState::Thinking => Some(AgentTaskState::Thinking),
        AgentHookTaskState::RunningTool { tool_name } => {
            Some(AgentTaskState::RunningTool { tool_name })
        }
        AgentHookTaskState::WaitingApproval { tool_name } => {
            Some(AgentTaskState::WaitingApproval { tool_name })
        }
        AgentHookTaskState::Done => Some(AgentTaskState::Done),
        AgentHookTaskState::Failed { message } => Some(AgentTaskState::Failed { message }),
        AgentHookTaskState::Ended => Some(AgentTaskState::Ended),
    }
}

fn canvas_profile(profile: norma::protocol::AgentProfile) -> CanvasAgentProfile {
    match profile {
        norma::protocol::AgentProfile::Claude => CanvasAgentProfile::Claude,
        norma::protocol::AgentProfile::Codex => CanvasAgentProfile::Codex,
        norma::protocol::AgentProfile::Cursor => CanvasAgentProfile::Cursor,
        norma::protocol::AgentProfile::Grok => CanvasAgentProfile::Grok,
        norma::protocol::AgentProfile::OpenCode => CanvasAgentProfile::OpenCode,
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
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
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ((nanos >> ((index % 16) * 8)) & 0xff) as u8;
    }
    bytes
}

#[derive(Debug)]
enum AgentHookHttpError {
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

impl From<serde_json::Error> for AgentHookHttpError {
    fn from(_error: serde_json::Error) -> Self {
        Self::Json
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use canvas::{AgentSpec, CanvasRect};
    use norma::agent_hook::parse_hook_event;

    /// 合法 hook event 应只更新对应 Agent node 的 task state。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   Agent node_id=1，profile=Codex
    ///   raw.hook_event_name="UserPromptSubmit" → Thinking
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. Runtime document 加入 Codex Agent node
    ///   2. parse_hook_event(Codex, node_id, raw)
    ///   3. apply_agent_hook_event
    ///
    /// 预期结果：
    ///   - 正断言：apply 返回 true，runtime task state=Thinking
    ///   - 负断言：不会写入 node_id+1 的状态
    #[test]
    fn test_apply_agent_hook_event_updates_target_agent_only() {
        let mut runtime = Runtime::new();
        let node_id = runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: CanvasAgentProfile::Codex,
                alias: None,
                definition_id: None,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        );
        let event = parse_hook_event(
            norma::agent_hook::HookSource::Codex,
            node_id,
            serde_json::json!({"hook_event_name": "UserPromptSubmit"}),
        )
        .expect("hook event 应可解析");

        let changed = apply_agent_hook_event(&mut runtime, event);

        assert!(changed, "合法 hook event 应更新 runtime task state");
        assert_eq!(
            runtime.agent_task_state(node_id),
            Some(AgentTaskState::Thinking),
            "目标 Agent node 应进入 Thinking"
        );
        assert_eq!(
            runtime.agent_task_state(node_id + 1),
            None,
            "非目标 node 不应被写入 task state"
        );
    }

    /// hook event 的 session_id 应透传到 runtime，避免关闭窗口时重新扫描 agent 会话目录。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   Agent node_id=1，profile=Codex
    ///   raw.session_id="hook-session-123"，由 Codex/Claude hook JSON 提供
    ///   raw.hook_event_name="UserPromptSubmit" → Thinking
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. Runtime document 加入 Codex Agent node
    ///   2. parse_hook_event 解析包含 session_id 的 raw JSON
    ///   3. apply_agent_hook_event 写入 runtime
    ///
    /// 预期结果：
    ///   - 正断言：runtime.agent_session_id(node_id)=Some("hook-session-123")
    ///   - 正断言：runtime task state=Thinking，说明 hook 正常生效
    ///   - 负断言：session_id 不应为 None，否则关闭路径仍会做昂贵 discovery
    #[test]
    fn test_apply_agent_hook_event_caches_session_id() {
        let mut runtime = Runtime::new();
        let node_id = runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: CanvasAgentProfile::Codex,
                alias: None,
                definition_id: None,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 400.0),
        );
        let event = parse_hook_event(
            norma::agent_hook::HookSource::Codex,
            node_id,
            serde_json::json!({
                "hook_event_name": "UserPromptSubmit",
                "session_id": "hook-session-123"
            }),
        )
        .expect("hook event 应可解析");

        let changed = apply_agent_hook_event(&mut runtime, event);

        assert!(changed, "合法 hook event 应更新 runtime task state");
        assert_eq!(
            runtime.agent_task_state(node_id),
            Some(AgentTaskState::Thinking),
            "目标 Agent node 应进入 Thinking"
        );
        assert_eq!(
            runtime.agent_session_id(node_id).as_deref(),
            Some("hook-session-123"),
            "hook session_id 应被 runtime 缓存，供 node_sessions 快速保存"
        );
        assert!(
            runtime.agent_session_id(node_id).is_some(),
            "session_id 不应丢失，否则关闭窗口会重新扫描 ~/.codex 或 ~/.claude"
        );
    }

    /// HTTP 请求 token 不匹配时应拒绝，不能产生 AgentHookEvent。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   expected token="good"
    ///   request token="bad"
    ///   body 是合法 AgentHookEvent JSON，Content-Length 等于 body byte 长度
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造一条 HTTP POST request
    ///   2. 调用 read_authorized_event
    ///
    /// 预期结果：
    ///   - 正断言：返回 Unauthorized
    ///   - 负断言：不应返回 Ok(event)
    #[test]
    fn test_read_authorized_event_rejects_invalid_token() {
        let body = serde_json::to_vec(
            &parse_hook_event(
                norma::agent_hook::HookSource::Codex,
                7,
                serde_json::json!({"hook_event_name": "Stop"}),
            )
            .expect("hook event 应可构造"),
        )
        .expect("hook event 应可序列化");
        let request = format!(
            "POST /agent-hooks HTTP/1.1\r\nAuthorization: Bearer bad\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(&body)
        );
        let (mut client, server) = test_tcp_pair();
        client
            .write_all(request.as_bytes())
            .expect("测试请求应可写入 socket");

        let result = read_authorized_event(&server, "good");

        assert!(
            matches!(result, Err(AgentHookHttpError::Unauthorized)),
            "token 不匹配应返回 Unauthorized，实际: {:?}",
            result
        );
        assert!(result.is_err(), "invalid token 不应解析出 event");
    }

    fn test_tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("测试 TCP listener 应可绑定");
        let addr = listener.local_addr().expect("测试 listener 应有地址");
        let client = TcpStream::connect(addr).expect("测试 client 应可连接");
        let (server, _) = listener.accept().expect("测试 server 应可 accept");
        (client, server)
    }
}
