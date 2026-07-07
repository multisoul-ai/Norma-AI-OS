use super::app_realtime_tests_chat_support::{
    chunk, config, with_realtime_process_guard, RecordingSidecar,
};
use super::card_tasks::{build_realtime_card_task_planning_request, DEFAULT_MAX_CARD_TASK_STEPS};
use crate::app::MultisoulApp;
use card_task::{CardTaskPlanner, CardTaskPlannerError, CardTaskRequest};
use gpui::TestAppContext;
use norma::protocol::{CardCommand, CardKind, Placement};
use realtime::{RealtimeCommand, RealtimeEvent, RealtimeToolResult};
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

fn create_terminal_plan() -> CompiledVoicePlan {
    CompiledVoicePlan {
        commands: vec![ExecutableCommand::Card(CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        })],
        requires_confirmation: false,
        risk: RiskLevel::Low,
        reason: Some("test".to_string()),
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

/// ARC-CB-1：task 终态在 realtime idle 时应立刻转换成 ToolResult 并发给 sidecar。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id   = "rt_callback_idle"
///   tool_call_id = "call_idle"
///   compiled plan = 1 步 Create Terminal，因此第 2 次 drain 后 task 必然 Succeeded
///   speaking flags = user=false, assistant=false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 RecordingSidecar 启动 realtime shell，并注入 1 个成功 planner
///   2. 调用 enqueue_realtime_card_task(...)
///   3. 连续 drain_realtime_card_tasks(...) 两次，完成 planning + execute
///   4. 读取 sidecar commands 中的 ToolResult
///
/// 预期结果：
///   - 正断言：生成 1 条 status=succeeded 的 ToolResult
///   - 正断言：ToolResult 携带 task_id 和 summary
///   - 负断言：idle completion 不应等待 transcript final / audio end 才发送
#[gpui::test]
async fn test_realtime_card_task_idle_completion_injects_tool_result_immediately(
    cx: &mut TestAppContext,
) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let tool_results = app.update_in(cx, |app, window, _cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_callback_idle"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar 应能启动测试 realtime shell");
            let _ = app.realtime.drain_sidecar_events();
            app.realtime
                .card_tasks_for_tests()
                .set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(vec![
                    Ok(create_terminal_plan()),
                ])));

            app.enqueue_realtime_card_task(
                "创建一个 Terminal 卡片".to_string(),
                "call_idle".to_string(),
                _cx,
            );
            let _ = app.drain_realtime_card_tasks(window, _cx);
            let _ = app.drain_realtime_card_tasks(window, _cx);
            tool_results(sidecar.commands())
        });

        assert_eq!(
            tool_results.len(),
            1,
            "idle task completion 应立即向 sidecar 发送一条 ToolResult"
        );
        assert_eq!(
            tool_results[0].result["status"],
            serde_json::json!("succeeded"),
            "成功任务必须编码为 status=succeeded"
        );
        assert_eq!(
            tool_results[0].result["task_id"],
            serde_json::json!("task_0"),
            "首个任务完成时 ToolResult 必须带回 task_id=task_0"
        );
        assert!(
            tool_results[0].result["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("Terminal")),
            "成功 ToolResult 应带回可读 summary，便于 Realtime 自然汇报结果"
        );
    });
}

/// ARC-CB-2：user 正在说话时 task completion 先排队，直到 transcript final 再注入 ToolResult。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id   = "rt_callback_user"
///   tool_call_id = "call_user"
///   user_speaking = true（通过 send_input_audio_chunk 模拟）
///   compiled plan = 1 步 Create Terminal
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 RecordingSidecar realtime shell，并让 planner 返回成功 plan
///   2. 调用 send_input_audio_chunk(...) 把 session 置为 user_speaking=true
///   3. 执行 task 到终态，读取 sidecar commands
///   4. 注入 TranscriptFinal 事件并 drain_realtime_sidecar(...)
///   5. 再次读取 sidecar commands 中的 ToolResult
///
/// 预期结果：
///   - 正断言：用户说话期间 ToolResult 数量为 0
///   - 正断言：TranscriptFinal 后出现 1 条 ToolResult
///   - 负断言：queued callback 不应在用户说完前抢话
#[gpui::test]
async fn test_realtime_card_task_user_speaking_queues_until_transcript_final(
    cx: &mut TestAppContext,
) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (before_flush, after_flush) = app.update_in(cx, |app, window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_callback_user"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar 应能启动测试 realtime shell");
            let _ = app.realtime.drain_sidecar_events();
            app.realtime
                .card_tasks_for_tests()
                .set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(vec![
                    Ok(create_terminal_plan()),
                ])));
            app.realtime
                .send_input_audio_chunk(chunk(vec![0, 1]))
                .expect("send_input_audio_chunk 应能把 session 置为 user speaking");

            app.enqueue_realtime_card_task(
                "创建一个 Terminal 卡片".to_string(),
                "call_user".to_string(),
                cx,
            );
            let _ = app.drain_realtime_card_tasks(window, cx);
            let _ = app.drain_realtime_card_tasks(window, cx);
            let before_flush = tool_results(sidecar.commands());

            app.realtime
                .apply_sidecar_event_for_tests(RealtimeEvent::TranscriptFinal {
                    session_id: realtime::RealtimeSessionId::new("rt_callback_user"),
                    text: "创建一个 Terminal 卡片".to_string(),
                });
            let after_flush = tool_results(sidecar.commands());
            (before_flush, after_flush)
        });

        assert!(
            before_flush.is_empty(),
            "user speaking 时 task completion 不应立刻生成 ToolResult"
        );
        assert_eq!(
            after_flush.len(),
            1,
            "TranscriptFinal 到来后应 flush 1 条排队的 ToolResult"
        );
        assert_eq!(
            after_flush[0].result["status"],
            serde_json::json!("succeeded"),
            "flush 后的 ToolResult 仍应保持成功状态"
        );
    });
}

/// ARC-CB-3：assistant 正在播报时 task completion 先排队，assistant audio end 后再注入 ToolResult。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id   = "rt_callback_assistant"
///   tool_call_id = "call_assistant"
///   assistant_speaking = true（通过 AssistantAudioState(speaking=true) 模拟）
///   compiled plan = 1 步 Create Terminal
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 RecordingSidecar realtime shell，并把 session 置为 assistant speaking
///   2. 执行 task 到终态，读取 sidecar commands
///   3. 注入 AssistantAudioState(speaking=false) 并 drain_realtime_sidecar(...)
///   4. 再读取 ToolResult commands
///
/// 预期结果：
///   - 正断言：assistant speaking 期间 ToolResult 数量为 0
///   - 正断言：assistant audio end 后出现 1 条 ToolResult
///   - 负断言：queued callback 不应覆盖当前 assistant 播报
#[gpui::test]
async fn test_realtime_card_task_assistant_speaking_queues_until_audio_end(
    cx: &mut TestAppContext,
) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (before_flush, after_flush) = app.update_in(cx, |app, window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_callback_assistant"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar 应能启动测试 realtime shell");
            let _ = app.realtime.drain_sidecar_events();
            sidecar.push_event(Ok(RealtimeEvent::AssistantAudioState {
                session_id: realtime::RealtimeSessionId::new("rt_callback_assistant"),
                speaking: true,
            }));
            let _ = app.drain_realtime_sidecar(cx);
            app.realtime
                .card_tasks_for_tests()
                .set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(vec![
                    Ok(create_terminal_plan()),
                ])));

            app.enqueue_realtime_card_task(
                "创建一个 Terminal 卡片".to_string(),
                "call_assistant".to_string(),
                cx,
            );
            let _ = app.drain_realtime_card_tasks(window, cx);
            let _ = app.drain_realtime_card_tasks(window, cx);
            let before_flush = tool_results(sidecar.commands());

            sidecar.push_event(Ok(RealtimeEvent::AssistantAudioState {
                session_id: realtime::RealtimeSessionId::new("rt_callback_assistant"),
                speaking: false,
            }));
            let _ = app.drain_realtime_sidecar(cx);
            let after_flush = tool_results(sidecar.commands());
            (before_flush, after_flush)
        });

        assert!(
            before_flush.is_empty(),
            "assistant speaking 时 task completion 不应立刻发 ToolResult 抢占当前播报"
        );
        assert_eq!(
            after_flush.len(),
            1,
            "assistant audio end 后应 flush 1 条排队的 ToolResult"
        );
        assert_eq!(
            after_flush[0].result["status"],
            serde_json::json!("succeeded"),
            "assistant audio end flush 后 ToolResult 应保持成功状态"
        );
    });
}

/// ARC-CB-4：realtime 不可用或 tool_call_id 缺失时应只跳过注入，不 panic，也不生成无效 ToolResult。
///
/// 数据构造（含关键数值的推导过程）：
///   case A = 未启动 realtime shell，但直接执行 1 条成功 task
///   case B = 已启动 realtime shell，但直接 enqueue 1 条 tool_call_id=None 的成功 task
///   两个 case 都会真正完成 task，因此都要走到 Task 5 callback 桥接点
///
/// 执行过程（逐步说明系统如何处理）：
///   1. case A：不启动 realtime，直接设置 planner 并执行 task 到终态
///   2. case B：启动 RecordingSidecar，使用 tool_call_id=None 的测试入口执行 task 到终态
///   3. 收集 case B 的 sidecar ToolResult commands
///
/// 预期结果：
///   - 正断言：两个 case 都能完成执行，不 panic
///   - 正断言：缺少 tool_call_id 的 case 不会生成 ToolResult
///   - 负断言：realtime unavailable / missing tool_call_id 都不应把 shell 置为 Realtime error
#[gpui::test]
async fn test_realtime_card_task_callback_skips_when_realtime_unavailable_or_tool_call_id_missing(
    cx: &mut TestAppContext,
) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (offline_created, missing_id_results, label) = app.update_in(cx, |app, window, cx| {
            app.realtime
                .card_tasks_for_tests()
                .set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(vec![
                    Ok(create_terminal_plan()),
                    Ok(create_terminal_plan()),
                ])));

            app.enqueue_realtime_card_task(
                "离线创建 Terminal".to_string(),
                "call_offline".to_string(),
                cx,
            );
            let _ = app.drain_realtime_card_tasks(window, cx);
            let _ = app.drain_realtime_card_tasks(window, cx);
            let offline_created = app.runtime.document.node(1).is_some();

            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_callback_missing_id"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar 应能启动测试 realtime shell");
            let _ = app.realtime.drain_sidecar_events();

            let planning_request = build_realtime_card_task_planning_request(
                &app.runtime,
                "没有 tool_call_id".to_string(),
                DEFAULT_MAX_CARD_TASK_STEPS,
            );
            app.realtime.card_tasks_for_tests().enqueue_task_for_tests(
                "没有 tool_call_id".to_string(),
                None,
                planning_request,
            );
            let _ = app.drain_realtime_card_tasks(window, cx);
            let _ = app.drain_realtime_card_tasks(window, cx);

            (
                offline_created,
                tool_results(sidecar.commands()),
                app.effective_voice_label(),
            )
        });

        assert!(
            offline_created,
            "realtime unavailable 时 task 仍应能完成自身执行，而不是因为 callback 注入失败而中断"
        );
        assert!(
            missing_id_results.is_empty(),
            "tool_call_id 缺失时不应生成无法对应 OpenAI function_call_output 的 ToolResult"
        );
        assert_ne!(
            label, "Realtime error",
            "realtime unavailable / missing tool_call_id 都不应把 shell 置为 Realtime error"
        );
    });
}

/// ARC-CB-5：session_id 仍存在但 sidecar 已错误退出时，card task callback 必须直接丢弃，不能滞留在 pending queue。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id   = "rt_callback_sidecar_error"
///   tool_call_id = "call_sidecar_error"
///   compiled plan = 1 步 Create Terminal，因此第 2 次 drain 后 task 必然 Succeeded
///   sidecar error = RealtimeSidecarError::MalformedEvent，会触发 mark_sidecar_error，但保留 session_id
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 RecordingSidecar realtime shell，并先 drain 到 SessionStarted
///   2. 注入 MalformedEvent 错误并 drain_realtime_sidecar(...)，把 realtime 置为 error / sidecar unavailable
///   3. 在该错误态下执行 1 条成功 task 到终态
///   4. 读取 sidecar commands 中的 ToolResult，并查询 pending callback 数量
///
/// 预期结果：
///   - 正断言：sidecar error 后 label 显示 Realtime error
///   - 正断言：错误态完成 task 后不会新增任何 ToolResult
///   - 正断言：pending callback queue 长度为 0
///   - 负断言：session_id 残留时也不应把 callback 偷偷排进 queue，等待未来错误恢复
#[gpui::test]
async fn test_realtime_card_task_callback_drops_when_session_id_exists_but_sidecar_is_unavailable(
    cx: &mut TestAppContext,
) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (label, tool_results_after_error, pending_callbacks) =
            app.update_in(cx, |app, window, cx| {
                app.realtime
                    .start_shell_with_sidecar_for_tests(
                        config("rt_callback_sidecar_error"),
                        Box::new(sidecar.clone()),
                    )
                    .expect("recording sidecar 应能启动测试 realtime shell");
                let _ = app.realtime.drain_sidecar_events();

                sidecar.push_event(Err(realtime::RealtimeSidecarError::MalformedEvent));
                let _ = app.drain_realtime_sidecar(cx);

                app.realtime
                    .card_tasks_for_tests()
                    .set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(
                        vec![Ok(create_terminal_plan())],
                    )));

                app.enqueue_realtime_card_task(
                    "sidecar error 后创建 Terminal".to_string(),
                    "call_sidecar_error".to_string(),
                    cx,
                );
                let _ = app.drain_realtime_card_tasks(window, cx);
                let _ = app.drain_realtime_card_tasks(window, cx);

                (
                    app.effective_voice_label(),
                    tool_results(sidecar.commands()),
                    app.realtime.pending_tool_result_count_for_tests(),
                )
            });

        assert_eq!(
            label, "Realtime error",
            "sidecar error 注入后 realtime 应进入错误态，确保测试命中 session_id 仍在但 session unavailable 的窗口"
        );
        assert!(
            tool_results_after_error.is_empty(),
            "session_id 仍存在但 sidecar unavailable 时不应发送任何 ToolResult"
        );
        assert_eq!(
            pending_callbacks, 0,
            "session unavailable 时 callback 必须直接丢弃，pending queue 中不应残留未来无法 flush 的项"
        );
    });
}
