use card_task::{
    CardCommandRunner, CardTaskEvent, CardTaskPlanner, CardTaskPlannerError, CardTaskQueue,
    CardTaskRequest,
};
use norma::protocol::{
    AgentProfile, CardCommand, CardCommandError, CardCommandResult, CardKind, Placement,
};
use realtime::{
    decode_event, encode_command, RealtimeCommand, RealtimeEvent, RealtimeSessionId,
    RealtimeSessionState, RealtimeToolResult, RealtimeToolResultStatus,
};
use serde_json::{json, Value};
use voice::{CompiledVoicePlan, ExecutableCommand, RiskLevel};

struct StaticPlanner {
    plan: Option<CompiledVoicePlan>,
}

impl StaticPlanner {
    fn create_then_send_readme() -> Self {
        Self {
            plan: Some(CompiledVoicePlan {
                commands: vec![
                    ExecutableCommand::Card(CardCommand::Create {
                        kind: CardKind::Agent {
                            profile: AgentProfile::Codex,
                        },
                        placement: Placement::VisibleArea,
                    }),
                    ExecutableCommand::SendTextToPrevious {
                        previous_step: 0,
                        text: "请分析 README".to_string(),
                        submit: true,
                    },
                ],
                requires_confirmation: false,
                risk: RiskLevel::Low,
                reason: Some("create Codex and send README analysis request".to_string()),
            }),
        }
    }
}

impl CardTaskPlanner for StaticPlanner {
    fn plan(
        &mut self,
        _request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
        self.plan
            .take()
            .ok_or_else(|| CardTaskPlannerError::Provider {
                message: "planner called more than once".to_string(),
            })
    }
}

#[derive(Default)]
struct RecordingRunner {
    commands: Vec<CardCommand>,
}

impl CardCommandRunner for RecordingRunner {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        self.commands.push(command.clone());
        match command {
            CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 77 }),
            CardCommand::SendText { node_id, .. } => Ok(CardCommandResult::Sent { node_id }),
            CardCommand::Focus { node_id } => Ok(CardCommandResult::Focused { node_id }),
            CardCommand::Delete { node_id } => Ok(CardCommandResult::Deleted { node_id }),
            CardCommand::List => Ok(CardCommandResult::Listed { cards: Vec::new() }),
        }
    }
}

fn decode_tool_call_line() -> realtime::RealtimeToolCall {
    let line = serde_json::to_string(&json!({
        "type": "tool_call",
        "session_id": "rt_e2e",
        "tool_call_id": "call_e2e_1",
        "name": "enqueue_card_task",
        "arguments": {
            "intent": "创建一个 Codex 卡片，让它分析 README，然后发送"
        }
    }))
    .expect("测试 JSONL 应可序列化");

    match decode_event(&line).expect("tool_call JSONL 应可被 realtime 协议解码") {
        RealtimeEvent::ToolCall(call) => call,
        other => panic!("期望解码出 ToolCall，实际得到 {other:?}"),
    }
}

fn succeeded_result_from_events(
    session_id: RealtimeSessionId,
    tool_call_id: &str,
    events: &[CardTaskEvent],
) -> RealtimeToolResult {
    let succeeded = events
        .iter()
        .find_map(|event| match event {
            CardTaskEvent::Succeeded { task_id, summary } => Some((task_id, summary)),
            _ => None,
        })
        .expect("任务事件流必须包含 Succeeded，才能回灌 realtime tool_result");

    RealtimeToolResult::task_result(
        session_id,
        tool_call_id,
        RealtimeToolResultStatus::Succeeded,
        succeeded.0.to_string(),
        succeeded.1.clone(),
    )
}

/// E2E-RT-1：Realtime tool_call 到 card_task create->send，再到 callback tool_result 的跨 crate 闭环。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id       = "rt_e2e"
///   tool_call_id     = "call_e2e_1"
///   intent           = "创建一个 Codex 卡片，让它分析 README，然后发送"
///   plan step0       = Create Agent(Codex)
///   fake created id  = 77（runner 对 Create 固定返回）
///   plan step1       = SendTextToPrevious(step0, "请分析 README", submit=true)
///   callback limit   = default 16，本用例只入队 1 条，因此不会触发 drop
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 realtime::decode_event 解码 JSONL tool_call，得到 enqueue_card_task(intent)
///   2. 把 intent/tool_call_id 入 CardTaskQueue，并用 StaticPlanner 生成 create->send plan
///   3. 连续 run_next 两次：第一次执行 Create，第二次解析 previous step 并执行 SendText
///   4. 把 Succeeded 事件转换为 RealtimeToolResult，assistant speaking=true 时先 queue
///   5. assistant audio end 后 flush pending callback，并用 realtime::encode_command 编码 JSONL
///
/// 预期结果：
///   - 正断言：tool_call name 是 enqueue_card_task，intent 原样保留
///   - 正断言：runner 精确执行 Create Agent(Codex) -> SendText(node_id=77, submit=true)
///   - 正断言：assistant speaking 时 result 不注入，pending queue 为 1
///   - 正断言：audio end 后注入 1 条 status=succeeded 的 tool_result JSONL
///   - 负断言：执行过程中不应出现 Failed / Cancelled 终态
#[test]
fn test_realtime_tool_call_create_send_and_callback_pipeline() {
    let call = decode_tool_call_line();
    assert_eq!(
        call.name, "enqueue_card_task",
        "JSONL tool_call 应解码为 enqueue_card_task，而不是其他工具名"
    );
    assert_eq!(
        call.arguments["intent"],
        json!("创建一个 Codex 卡片，让它分析 README，然后发送"),
        "tool_call arguments.intent 必须原样传入 card task request"
    );

    let mut queue = CardTaskQueue::default();
    let task_id = queue.enqueue(CardTaskRequest {
        intent: call.arguments["intent"]
            .as_str()
            .expect("intent 应是字符串")
            .to_string(),
        tool_call_id: Some(call.tool_call_id.clone()),
    });
    let queued_events = queue.drain_events();
    assert_eq!(
        queued_events,
        vec![CardTaskEvent::Queued {
            task_id,
            summary: "创建一个 Codex 卡片，让它分析 README，然后发送".to_string(),
        }],
        "enqueue 后必须只产生一个 Queued 事件，证明 task_id/summary 进入队列"
    );

    let mut planner = StaticPlanner::create_then_send_readme();
    let mut runner = RecordingRunner::default();
    let first_events = queue.run_next(&mut planner, &mut runner);
    let second_events = queue.run_next(&mut planner, &mut runner);

    assert_eq!(
        runner.commands,
        vec![
            CardCommand::Create {
                kind: CardKind::Agent {
                    profile: AgentProfile::Codex,
                },
                placement: Placement::VisibleArea,
            },
            CardCommand::SendText {
                node_id: 77,
                text: "请分析 README".to_string(),
                submit: true,
            },
        ],
        "card_task runner 必须精确执行 Create Agent(Codex) 后 SendText 到 step0 创建的 node_id=77"
    );

    assert!(
        first_events
            .iter()
            .any(|event| matches!(event, CardTaskEvent::Planned { step_count: 2, .. })),
        "第一次 run_next 必须完成 planning 并得到 2 步计划"
    );
    assert!(
        second_events.iter().any(
            |event| matches!(event, CardTaskEvent::Succeeded { task_id: id, .. } if *id == task_id)
        ),
        "第二次 run_next 必须让同一个 task_id 进入 Succeeded 终态"
    );
    assert!(
        !first_events.iter().any(|event| matches!(
            event,
            CardTaskEvent::Failed { .. } | CardTaskEvent::Cancelled { .. }
        )),
        "第一次 run_next 不应产生 Failed 或 Cancelled 终态"
    );
    assert!(
        !second_events.iter().any(|event| matches!(
            event,
            CardTaskEvent::Failed { .. } | CardTaskEvent::Cancelled { .. }
        )),
        "第二次 run_next 不应产生 Failed 或 Cancelled 终态"
    );

    let mut session = RealtimeSessionState::default();
    let session_id = RealtimeSessionId::new("rt_e2e");
    session
        .start_connecting(session_id.clone())
        .expect("新的 e2e realtime session 应能进入 connecting");
    session.mark_started();
    session.mark_assistant_speaking(true);

    let tool_result =
        succeeded_result_from_events(session_id.clone(), &call.tool_call_id, &second_events);
    let queued_update = session.queue_or_inject_tool_result(tool_result);
    assert!(
        queued_update.injected.is_empty(),
        "assistant speaking=true 时不应立即注入 ToolResult"
    );
    assert_eq!(
        session.pending_tool_result_count(),
        1,
        "assistant speaking=true 时 ToolResult 应进入 pending callback queue"
    );

    let flushed_update = session.handle_assistant_audio_end();
    assert_eq!(
        flushed_update.injected.len(),
        1,
        "assistant audio end 后应 flush 出 1 条 pending ToolResult"
    );
    assert!(
        flushed_update.dropped.is_empty(),
        "本用例只排队 1 条 callback，不应触发 queue drop"
    );
    assert_eq!(
        session.pending_tool_result_count(),
        0,
        "audio end flush 后 pending callback queue 应清空"
    );

    let encoded = encode_command(&RealtimeCommand::ToolResult(
        flushed_update.injected[0].clone(),
    ))
    .expect("RealtimeToolResult 应可编码为 JSONL command");
    let encoded_json: Value =
        serde_json::from_str(&encoded).expect("编码后的 tool_result JSONL 应是合法 JSON");

    assert_eq!(
        encoded_json["type"],
        json!("tool_result"),
        "回灌给 sidecar 的命令类型必须是 tool_result"
    );
    assert_eq!(
        encoded_json["session_id"],
        json!("rt_e2e"),
        "tool_result 必须回到原 realtime session"
    );
    assert_eq!(
        encoded_json["tool_call_id"],
        json!("call_e2e_1"),
        "tool_result 必须回到原 tool_call_id"
    );
    assert_eq!(
        encoded_json["result"]["status"],
        json!("succeeded"),
        "成功任务回灌 status 必须是 succeeded"
    );
    assert_eq!(
        encoded_json["result"]["task_id"],
        json!("task_0"),
        "首个入队任务的 task_id 应编码为 task_0"
    );
    assert!(
        encoded_json["result"]["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("创建一个 Codex 卡片")),
        "tool_result summary 应保留用户可理解的任务摘要，供 Realtime 自然汇报"
    );
}
