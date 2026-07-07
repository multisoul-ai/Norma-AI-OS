use super::app_realtime_tests_chat_support::{
    config, with_realtime_process_guard, RecordingSidecar,
};
use super::card_tasks::{build_realtime_card_task_planning_request, DEFAULT_MAX_CARD_TASK_STEPS};
use super::RealtimeControlState;
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsValues;
use card_task::{CardCommandRunner, CardTaskPlannerError, CardTaskRequest};
use gpui::TestAppContext;
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult};
use realtime::{RealtimeCommand, RealtimeEvent, RealtimeToolCall, RealtimeToolResult};
use serde_json::json;
use std::path::PathBuf;
use voice::{CompiledVoicePlan, ExecutableCommand, RiskLevel};

#[derive(Default)]
struct NoopRunner;

impl CardCommandRunner for NoopRunner {
    fn run(&mut self, _command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        Ok(CardCommandResult::Listed { cards: Vec::new() })
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

fn one_step_list_plan() -> CompiledVoicePlan {
    CompiledVoicePlan {
        commands: vec![ExecutableCommand::Card(CardCommand::List)],
        requires_confirmation: false,
        risk: RiskLevel::Low,
        reason: Some("list cards".to_string()),
    }
}

/// RTF-1：默认 realtime card-task host 在没有后台规划结果前应保持等待，不应同步报 provider unavailable。
///
/// 数据构造（含关键数值的推导过程）：
///   host = RealtimeCardTaskState::default()，不注入任何测试 planner
///   task count = 1，planning_request.max_steps = 8
///   runner = NoopRunner，因为本轮预期不会真正执行任何 CardCommand
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造默认 host，并用测试入口 enqueue 1 个带 tool_call_id 的 task
///   2. 先 drain 掉同步 Queued 事件，避免干扰本轮断言
///   3. 连续调用 run_next() 两次，记录 changed 与事件流
///
/// 预期结果：
///   - 正断言：没有后台规划结果时两次 run_next 都返回 false
///   - 正断言：事件流为空，不应立刻生成 Failed(provider unavailable)
///   - 负断言：host 不应把 task 直接推进到终态失败
#[test]
fn test_realtime_card_task_host_waits_for_background_plan_without_provider_failure() {
    let mut host = super::card_tasks::RealtimeCardTaskState::default();
    let planning_request = build_realtime_card_task_planning_request(
        &runtime::Runtime::new(),
        "创建一个 Codex 卡片".to_string(),
        DEFAULT_MAX_CARD_TASK_STEPS,
    );
    host.enqueue_task_for_tests(
        "创建一个 Codex 卡片".to_string(),
        Some("call_wait_for_planner".to_string()),
        planning_request,
    );
    let _ = host.drain_events();
    let mut runner = NoopRunner;

    let first_changed = host.run_next(&mut runner);
    let second_changed = host.run_next(&mut runner);
    let events = host.drain_events();

    assert!(
        !first_changed,
        "没有后台规划结果前第一次 run_next 应保持等待，而不是同步推进失败"
    );
    assert!(
        !second_changed,
        "没有后台规划结果前第二次 run_next 仍应保持等待，不能因为 snapshot 被消耗而失败"
    );
    assert!(
        events.is_empty(),
        "没有后台规划结果前不应发出 Failed(provider unavailable) 之类的终态事件"
    );
}

/// RTF-1b：后台规划结果 Ready 后，默认 host 应继续执行原 task，而不是停在 Pending。
///
/// 数据构造（含关键数值的推导过程）：
///   task count = 1
///   pending ticks = 1（第一次 run_next 返回 Pending，不产生事件）
///   ready plan commands = 1（List，无副作用，NoopRunner 可执行）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用默认 host enqueue 1 个 task，并先 drain 掉 Queued
///   2. 第一次 run_next 触发 Pending，确认没有事件
///   3. 注入后台 Ready(CompiledVoicePlan) 结果
///   4. 第二次 run_next 消费 ready 结果并执行 List
///
/// 预期结果：
///   - 正断言：Ready 后 run_next 返回 true
///   - 正断言：事件流包含 Planned 与 Succeeded
///   - 负断言：Ready 后不应出现 provider unavailable / missing snapshot 的 Failed
#[test]
fn test_realtime_card_task_host_consumes_ready_background_plan_after_pending() {
    let mut host = super::card_tasks::RealtimeCardTaskState::default();
    let request = CardTaskRequest {
        intent: "列出卡片".to_string(),
        tool_call_id: Some("call_ready_plan".to_string()),
    };
    let planning_request = build_realtime_card_task_planning_request(
        &runtime::Runtime::new(),
        request.intent.clone(),
        DEFAULT_MAX_CARD_TASK_STEPS,
    );
    host.enqueue_task_for_tests(
        request.intent.clone(),
        request.tool_call_id.clone(),
        planning_request,
    );
    let _ = host.drain_events();
    let mut runner = NoopRunner;

    let pending_changed = host.run_next(&mut runner);
    let pending_events = host.drain_events();
    host.finish_planning_for_tests(request, Ok(one_step_list_plan()));
    let ready_changed = host.run_next(&mut runner);
    let ready_events = host.drain_events();

    assert!(
        !pending_changed,
        "Ready 结果到达前第一次 run_next 应保持 Pending，不应产生前台变化"
    );
    assert!(
        pending_events.is_empty(),
        "Ready 结果到达前不应产生 Planning/Failed 等事件"
    );
    assert!(
        ready_changed,
        "Ready 结果到达后 run_next 应继续推进并返回 changed=true"
    );
    assert!(
        ready_events
            .iter()
            .any(|event| matches!(event, card_task::CardTaskEvent::Planned { .. })),
        "Ready 结果应产生 Planned 事件"
    );
    assert!(
        ready_events
            .iter()
            .any(|event| matches!(event, card_task::CardTaskEvent::Succeeded { .. })),
        "单步 List plan 执行后应产生 Succeeded 终态"
    );
    assert!(
        !ready_events.iter().any(|event| matches!(
            event,
            card_task::CardTaskEvent::Failed { message, .. }
                if message.contains("provider") || message.contains("snapshot")
        )),
        "Ready 后不应再退化成 provider unavailable 或 missing snapshot 失败"
    );
}

/// RTF-1c：后台规划失败结果应在 Ready/Failed 回写后才让 task 进入 Failed。
///
/// 数据构造（含关键数值的推导过程）：
///   task count = 1
///   pending ticks = 1
///   failure = CardTaskPlannerError::Provider("planner down")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enqueue task 并第一次 run_next 进入 Pending
///   2. 注入后台 Failed(planner down) 结果
///   3. 第二次 run_next 消费失败结果
///
/// 预期结果：
///   - 正断言：失败结果回写后才产生 Failed(planner down)
///   - 负断言：Pending 阶段不应提前失败
#[test]
fn test_realtime_card_task_host_consumes_background_planner_error_after_pending() {
    let mut host = super::card_tasks::RealtimeCardTaskState::default();
    let request = CardTaskRequest {
        intent: "创建卡片".to_string(),
        tool_call_id: Some("call_failed_plan".to_string()),
    };
    let planning_request = build_realtime_card_task_planning_request(
        &runtime::Runtime::new(),
        request.intent.clone(),
        DEFAULT_MAX_CARD_TASK_STEPS,
    );
    host.enqueue_task_for_tests(
        request.intent.clone(),
        request.tool_call_id.clone(),
        planning_request,
    );
    let _ = host.drain_events();
    let mut runner = NoopRunner;

    let pending_changed = host.run_next(&mut runner);
    let pending_events = host.drain_events();
    host.finish_planning_for_tests(
        request,
        Err(CardTaskPlannerError::Provider {
            message: "planner down".to_string(),
        }),
    );
    let failed_changed = host.run_next(&mut runner);
    let failed_events = host.drain_events();

    assert!(!pending_changed, "后台失败结果到达前 task 应保持 Pending");
    assert!(
        pending_events.is_empty(),
        "后台失败结果到达前不应提前产生 Failed"
    );
    assert!(
        failed_changed,
        "后台失败结果到达后 run_next 应产生 Failed 事件"
    );
    assert!(
        failed_events.iter().any(|event| matches!(
            event,
            card_task::CardTaskEvent::Failed { message, .. } if message.contains("planner down")
        )),
        "后台 provider error 必须转成 Failed(planner down)"
    );
}

/// RTF-2：validate_start_config 应把 realtime card task 的两个 tool 注册到 start_session config。
///
/// 数据构造（含关键数值的推导过程）：
///   api_key = Some("sk-live-planner")
///   realtime_model = Some("gpt-realtime-2")
///   card_planner_model = Some("gpt-card-planner")
///   sidecar_path = /tmp/fake-sidecar.mjs
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造满足 realtime 启动约束的 VoiceSettingsValues
///   2. 调用 validate_start_config()
///   3. 检查 start_session config.tools
///
/// 预期结果：
///   - 正断言：tools 精确包含 enqueue_card_task 与 cancel_card_task
///   - 负断言：tools 不应为空，否则 live realtime 永远不会收到 card task tool call
#[test]
fn test_realtime_validate_start_config_registers_card_task_tools() {
    let values = VoiceSettingsValues {
        ollama_endpoint: "http://127.0.0.1:11434".to_string(),
        planner_model: "qwen2.5".to_string(),
        refine_model: "qwen2.5".to_string(),
        paraformer_model_dir: None,
        input_device_id: None,
        trace_enabled: false,
        realtime_enabled: true,
        openai_api_key: Some("sk-live-planner".to_string()),
        realtime_model: Some("gpt-realtime-2".to_string()),
        card_planner_model: Some("gpt-card-planner".to_string()),
        sidecar_path: Some(PathBuf::from("/tmp/fake-sidecar.mjs")),
    };

    let config = RealtimeControlState::validate_start_config(&values)
        .expect("完整 realtime 配置应生成 start_session config");

    assert_eq!(
        config.tools,
        vec![
            "enqueue_card_task".to_string(),
            "cancel_card_task".to_string(),
        ],
        "start_session config 必须注册 enqueue_card_task/cancel_card_task 两个 tool"
    );
    assert!(
        !config.tools.is_empty(),
        "tool 列表不应为空，否则 live realtime 永远不会收到 card task tool call"
    );
}

/// RTF-3：没有可取消 task 时，cancel tool_call 应立即回 failed，而不是静默吞掉。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = "rt_tool_cancel_none"
///   cancel tool_call_id = "call_cancel_none"
///   queue 初始为空，因此不会命中任何 pending/active task
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 RecordingSidecar realtime shell，并先 drain 掉 session_started
///   2. 注入一条 cancel_card_task tool_call
///   3. drain sidecar 后读取 ToolResult
///
/// 预期结果：
///   - 正断言：返回 1 条 status=failed 的 ToolResult
///   - 正断言：tool_call_id 必须回给 call_cancel_none
///   - 负断言：空队列 cancel 不应伪装成成功，也不应偷偷入队 card task
#[gpui::test]
async fn test_realtime_tool_call_cancel_without_task_emits_failed_tool_result(
    cx: &mut TestAppContext,
) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (events, results) = app.update_in(cx, |app, _window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_tool_cancel_none"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar should accept start_session");
            let _ = app.drain_realtime_sidecar(cx);

            sidecar.push_event(Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: realtime::RealtimeSessionId::new("rt_tool_cancel_none"),
                tool_call_id: "call_cancel_none".to_string(),
                name: "cancel_card_task".to_string(),
                arguments: json!({ "scope": "latest" }),
            })));
            let _ = app.drain_realtime_sidecar(cx);

            (
                app.realtime.drain_card_task_events(),
                tool_results(sidecar.commands()),
            )
        });

        assert!(
            events.is_empty(),
            "空队列 cancel tool_call 不应偷偷生成任何 card task 事件"
        );
        assert_eq!(
            results.len(),
            1,
            "没有可取消 task 时 cancel tool_call 也必须立即收到 1 条失败回执"
        );
        assert_eq!(
            results[0].tool_call_id, "call_cancel_none",
            "失败回执必须回给 cancel 自己的 tool_call_id"
        );
        assert_eq!(
            results[0].result["status"],
            json!("failed"),
            "没有可取消 task 时 cancel tool_call 必须返回 status=failed"
        );
    });
}
