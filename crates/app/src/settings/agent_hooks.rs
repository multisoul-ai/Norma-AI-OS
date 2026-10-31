use canvas::AgentProfile as CanvasAgentProfile;
#[cfg(test)]
use canvas::CanvasNodeKind;
#[cfg(test)]
use crossbeam_channel::unbounded;
#[cfg(feature = "e2e-test-support")]
use crossbeam_channel::Sender;
use crossbeam_channel::{Receiver, Sender};
use gpui::App;
use norma::agent_hook::{
    extract_prompt_text, observe_conversation, task_state_for_provider_event, AgentHookEvent,
    AgentHookTaskState, ConversationObservationKind,
};
use norma::protocol::{AgentTaskResult, AgentTaskResultAvailability};
use runtime::{
    AgentHookRuntimeConfig, AgentTaskState, AgentTaskUpdate, AgentTaskUpdateOutcome, Runtime,
};
use std::cell::RefCell;
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::net::{TcpListener, TcpStream};

use super::agent_hook_ingress;

mod agent_hook_completion;

struct ProbeNote {
    node_id: u64,
    found: bool,
}
#[cfg(test)]
use super::agent_hook_ingress::{read_authorized_event, AgentHookHttpError};

pub(crate) struct AgentHookServer {
    receiver: Receiver<AgentHookEvent>,
    approvals: Receiver<norma::agent_hook::HookApproval>,
    questions: Receiver<norma::agent_hook::HookQuestion>,
    #[cfg(feature = "e2e-test-support")]
    event_sender: Sender<AgentHookEvent>,
    #[cfg(feature = "e2e-test-support")]
    event_tap: Option<Sender<AgentHookEvent>>,
    endpoint: Option<String>,
    token: Option<String>,
    completion: RefCell<norma::agent_hook::CompletionJudge>,
    probe_tx: Sender<ProbeNote>,
    probe_rx: Receiver<ProbeNote>,
}

#[derive(Debug, Default)]
pub(crate) struct AgentHookDrainReport {
    pub(crate) changed: bool,
    pub(crate) session_binding_changed: bool,
    pub(crate) started_tasks: Vec<AgentHookStartedTask>,
    pub(crate) ready_node_ids: Vec<u64>,
    pub(crate) terminal_tasks: Vec<AgentHookTerminalTask>,
    pub(crate) events: Vec<AgentHookEvent>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct AgentHookStartedTask {
    pub(crate) node_id: u64,
    pub(crate) task_title: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct AgentHookTerminalTask {
    pub(crate) node_id: u64,
    /// 发布这条终态时的状态。同一 drain 里随后的 SessionEnd 会把实时状态改成 Ended，
    /// 结算必须用这里的值，不能再读实时状态。
    pub(crate) settled_state: AgentTaskState,
    pub(crate) result: AgentTaskResult,
}

impl AgentHookServer {
    pub(crate) fn start(cx: &mut App) -> Self {
        let registration = agent_hook_ingress::register(cx);
        let (probe_tx, probe_rx) = crossbeam_channel::unbounded();
        Self {
            receiver: registration.receiver,
            approvals: registration.approvals,
            questions: registration.questions,
            #[cfg(feature = "e2e-test-support")]
            event_sender: registration.sender,
            #[cfg(feature = "e2e-test-support")]
            event_tap: None,
            endpoint: registration.endpoint,
            token: registration.token,
            completion: agent_hook_completion::fresh_judge(),
            probe_tx,
            probe_rx,
        }
    }

    pub(crate) fn unregister(&mut self, cx: &mut App) {
        if let Some(token) = self.token.take() {
            agent_hook_ingress::unregister(&token, cx);
        }
        self.endpoint = None;
    }

    pub(crate) fn rotate(&mut self, cx: &mut App) {
        self.unregister(cx);
        let registration = agent_hook_ingress::register(cx);
        self.receiver = registration.receiver;
        self.approvals = registration.approvals;
        self.questions = registration.questions;
        #[cfg(feature = "e2e-test-support")]
        {
            self.event_sender = registration.sender;
        }
        self.endpoint = registration.endpoint;
        self.token = registration.token;
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.receiver.is_empty() || !self.approvals.is_empty() || !self.questions.is_empty()
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn enqueue_event_for_e2e(&self, event: AgentHookEvent) -> Result<(), String> {
        self.event_sender
            .send(event)
            .map_err(|_| "Agent Hook e2e event queue is closed".to_string())
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn observe_events_for_e2e(&mut self) -> Receiver<AgentHookEvent> {
        // 256 events bounds one isolated probe; never steals events from the production receiver.
        let (sender, receiver) = crossbeam_channel::bounded(256);
        self.event_tap = Some(sender);
        receiver
    }

    pub(crate) fn runtime_config(&self) -> Option<AgentHookRuntimeConfig> {
        Some(AgentHookRuntimeConfig {
            endpoint: self.endpoint.clone()?,
            token: self.token.clone()?,
            workspace_id: String::new(),
            workspace_route_id: String::new(),
            norma_socket: String::new(),
        })
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn disable_for_test(&mut self) {
        self.endpoint = None;
        self.token = None;
    }

    pub(crate) fn drain_into_runtime(
        &self,
        runtime: &mut Runtime,
        project_id: Option<&str>,
    ) -> AgentHookDrainReport {
        self.drain_at(runtime, project_id, agent_hook_completion::unix_now_ms())
    }

    #[cfg(test)]
    pub(crate) fn drain_into_runtime_at(
        &self,
        runtime: &mut Runtime,
        project_id: Option<&str>,
        now_ms: u64,
    ) -> AgentHookDrainReport {
        self.drain_at(runtime, project_id, now_ms)
    }
}

fn task_result_from_hook_event(
    event: &AgentHookEvent,
    background_unsettled: bool,
) -> AgentTaskResult {
    let observation = observe_conversation(event);
    match observation {
        Some(observation)
            if observation.kind == ConversationObservationKind::CompleteAgentReply
                && observation.text.is_some() =>
        {
            AgentTaskResult {
                availability: AgentTaskResultAvailability::Available,
                text: observation.text,
                truncated: observation.truncated,
                background_unsettled,
            }
        }
        _ => AgentTaskResult {
            availability: AgentTaskResultAvailability::Unavailable,
            text: None,
            truncated: false,
            background_unsettled,
        },
    }
}

#[cfg(test)]
pub(crate) fn hook_server_for_test(receiver: Receiver<AgentHookEvent>) -> AgentHookServer {
    let (probe_tx, probe_rx) = crossbeam_channel::unbounded();
    #[cfg(feature = "e2e-test-support")]
    let (event_sender, _) = crossbeam_channel::unbounded();
    AgentHookServer {
        receiver,
        approvals: crossbeam_channel::bounded(1).1,
        questions: crossbeam_channel::bounded(1).1,
        #[cfg(feature = "e2e-test-support")]
        event_sender,
        #[cfg(feature = "e2e-test-support")]
        event_tap: None,
        endpoint: None,
        token: None,
        completion: agent_hook_completion::fresh_judge(),
        probe_tx,
        probe_rx,
    }
}

#[cfg(test)]
pub(crate) fn apply_agent_hook_event(runtime: &mut Runtime, event: AgentHookEvent) -> bool {
    apply_agent_hook_event_with_outcome(runtime, event).changed()
}

fn apply_agent_hook_event_with_outcome(
    runtime: &mut Runtime,
    event: AgentHookEvent,
) -> AgentTaskUpdateOutcome {
    if !runtime.accepts_mobile_hook(&event) {
        return AgentTaskUpdateOutcome::default();
    }
    let Some(to) = agent_task_state_from_hook_event(&event) else {
        tracing::debug!(provider_event = %event.provider_event, "ignored unmapped agent hook event");
        return AgentTaskUpdateOutcome::default();
    };
    let task_title = if event.provider_event == "UserPromptSubmit" {
        extract_prompt_text(&event.raw_json)
    } else {
        None
    };
    runtime.apply_agent_task_update_with_outcome(AgentTaskUpdate {
        node_id: event.node_id,
        profile: canvas_profile(event.profile),
        provider_event: event.provider_event,
        to,
        task_title,
        session_id: event.session_id,
    })
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
        norma::protocol::AgentProfile::InfCode => CanvasAgentProfile::InfCode,
        norma::protocol::AgentProfile::Kimi => CanvasAgentProfile::Kimi,
        norma::protocol::AgentProfile::ZCode => CanvasAgentProfile::ZCode,
        norma::protocol::AgentProfile::Pi => CanvasAgentProfile::Pi,
        norma::protocol::AgentProfile::Hermes => CanvasAgentProfile::Hermes,
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
                mobile_control: false,
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
                mobile_control: false,
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

#[cfg(test)]
#[path = "../../tests/unit/agent_hook_session_binding_tests.rs"]
mod session_binding_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent_hook_codex_host_tests.rs"]
mod codex_host_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent_hook_completion_tests.rs"]
mod completion_tests;
