use super::app_realtime_tests_chat_support::{
    config, with_realtime_process_guard, RecordingSidecar,
};
use crate::app::MultisoulApp;
use card_task::{CardCommandRunner, CardTaskPlanner, CardTaskPlannerError, CardTaskRequest};
use gpui::TestAppContext;
use norma::protocol::{
    AgentProfile, CardCommand, CardCommandError, CardCommandResult, CardKind, Placement,
};
use realtime::{RealtimeCommand, RealtimeEvent, RealtimeToolCall, RealtimeToolResult};
use serde_json::json;
use std::collections::VecDeque;
use voice::{CompiledVoicePlan, ExecutableCommand, RiskLevel};

#[derive(Default)]
struct FakeCompiledPlanner {
    results: VecDeque<Result<CompiledVoicePlan, CardTaskPlannerError>>,
}

impl FakeCompiledPlanner {
    fn with_results(results: Vec<Result<CompiledVoicePlan, CardTaskPlannerError>>) -> Self {
        Self {
            results: results.into(),
        }
    }
}

impl CardTaskPlanner for FakeCompiledPlanner {
    fn plan(
        &mut self,
        _request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError> {
        self.results
            .pop_front()
            .expect("fake compiled planner 应预先准备结果")
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

fn acceptance_plan() -> CompiledVoicePlan {
    CompiledVoicePlan {
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
        reason: Some("create codex and send README analysis request".to_string()),
    }
}

fn tool_results(commands: Vec<RealtimeCommand>) -> Vec<RealtimeToolResult> {
    commands
        .into_iter()
        .filter_map(|command| match command {
            RealtimeCommand::ToolResult(result) => Some(result),
            _ => None,
        })
        .collect()
}

/// AC-1：app-level 验收链必须覆盖 tool_call -> create Codex -> send README -> callback queue/inject。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id    = "rt_acceptance"
///   tool_call_id  = "call_acceptance_1"
///   intent        = "创建一个 Codex 卡片，让它分析 README，然后发送"
///   compiled plan = 2 步：step0 create Codex，step1 send_text(previous_step=0, submit=true)
///   fake created_id = 77，因此 send_text 应解析成 node_id=77
///   assistant_speaking = true，因此成功 ToolResult 先排队；audio end 后再注入 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 RecordingSidecar realtime shell，并先置为 assistant speaking=true
///   2. 注入 `enqueue_card_task` tool_call，验证 app 走 sidecar -> task queue 路径
///   3. 用 fake runner 连续 drain 两次，驱动 create Codex 与 send_text(previous_step)
///   4. 在 assistant 仍说话时检查 ToolResult 还未发送，但 pending queue=1
///   5. 注入 AssistantAudioState(speaking=false) 并 drain sidecar，触发 callback flush
///
/// 预期结果：
///   - 正断言：fake runner 命令顺序精确为 Create Agent(Codex) -> SendText(node_id=77, submit=true)
///   - 正断言：事件流包含 Queued 与 Succeeded
///   - 正断言：audio end 后生成 1 条 status=succeeded 的 ToolResult
///   - 负断言：assistant speaking 期间不应提前注入 ToolResult
#[gpui::test]
async fn test_realtime_acceptance_tool_call_executes_create_then_send_and_flushes_after_audio_end(
    cx: &mut TestAppContext,
) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (
            first_changed,
            second_changed,
            runner_commands,
            pending_before_flush,
            results_before_flush,
            results_after_flush,
            events,
        ) = app.update_in(cx, |app, _window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_acceptance"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar 应能启动测试 realtime shell");
            let _ = app.drain_realtime_sidecar(cx);
            sidecar.push_event(Ok(RealtimeEvent::AssistantAudioState {
                session_id: realtime::RealtimeSessionId::new("rt_acceptance"),
                speaking: true,
            }));
            let _ = app.drain_realtime_sidecar(cx);

            app.realtime
                .card_tasks_for_tests()
                .set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(vec![
                    Ok(acceptance_plan()),
                ])));

            sidecar.push_event(Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: realtime::RealtimeSessionId::new("rt_acceptance"),
                tool_call_id: "call_acceptance_1".to_string(),
                name: "enqueue_card_task".to_string(),
                arguments: json!({ "intent": "创建一个 Codex 卡片，让它分析 README，然后发送" }),
            })));
            let _ = app.drain_realtime_sidecar(cx);

            let mut runner = RecordingRunner::default();
            let first_changed = app.drain_realtime_card_tasks_with_runner_for_tests(&mut runner);
            let second_changed = app.drain_realtime_card_tasks_with_runner_for_tests(&mut runner);
            let pending_before_flush = app.realtime.pending_tool_result_count_for_tests();
            let results_before_flush = tool_results(sidecar.commands());

            sidecar.push_event(Ok(RealtimeEvent::AssistantAudioState {
                session_id: realtime::RealtimeSessionId::new("rt_acceptance"),
                speaking: false,
            }));
            let _ = app.drain_realtime_sidecar(cx);

            (
                first_changed,
                second_changed,
                runner.commands,
                pending_before_flush,
                results_before_flush,
                tool_results(sidecar.commands()),
                app.realtime.drain_card_task_events(),
            )
        });

        assert!(first_changed, "第一次 drain 应推进 planning + create step");
        assert!(
            second_changed,
            "第二次 drain 应推进 send_text step 并结束 task"
        );
        assert_eq!(
            runner_commands,
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
            "验收链中的 runner 命令序列必须是 Create Agent(Codex) 然后 SendText(node_id=77, submit=true)"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, card_task::CardTaskEvent::Queued { .. })),
            "tool_call 入队后事件流必须包含 Queued，证明命中了完整 app enqueue 链"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, card_task::CardTaskEvent::Succeeded { .. })),
            "create + send 执行完成后事件流必须包含 Succeeded"
        );
        assert!(
            results_before_flush.is_empty(),
            "assistant speaking 期间不应提前把成功 ToolResult 发给 sidecar"
        );
        assert_eq!(
            pending_before_flush, 1,
            "assistant speaking 期间成功结果应先进入 callback pending queue"
        );
        assert_eq!(
            results_after_flush.len(),
            1,
            "assistant audio end 后应 flush 出 1 条 ToolResult"
        );
        assert_eq!(
            results_after_flush[0].tool_call_id, "call_acceptance_1",
            "flush 后的 ToolResult 必须回到原始 realtime tool_call_id"
        );
        assert_eq!(
            results_after_flush[0].result["status"],
            json!("succeeded"),
            "成功 task 回灌 Realtime 时必须编码为 status=succeeded"
        );
        assert!(
            results_after_flush[0].result["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("创建一个 Codex 卡片")),
            "成功 ToolResult summary 应保留可读任务摘要，便于 assistant 自然汇报"
        );
    });
}
