use super::app_realtime_tests_chat_support::{
    config, with_realtime_process_guard, RecordingSidecar,
};
use crate::app::MultisoulApp;
use card_task::CardTaskEvent;
use gpui::TestAppContext;
use realtime::{RealtimeCommand, RealtimeEvent, RealtimeToolCall, RealtimeToolResult};
use serde_json::json;

/// RTC-1：sidecar `enqueue_card_task(intent)` tool_call 应路由到 app card-task 入队入口，而不是直接报错。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id    = "rt_tool_enqueue"
///   tool_call_id  = "call_enqueue_1"
///   intent        = "创建 Codex 卡片"
///   queue 初始为空，因此成功路由后应至少产生 1 条 Queued 事件
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 RecordingSidecar 启动一个测试 realtime shell，并先 drain 掉自动的 session_started
///   2. 向 fake sidecar 注入一条 ToolCall(enqueue_card_task)
///   3. 调用 app drain sidecar 入口，随后读取 card-task 事件和 sidecar commands
///
/// 预期结果：
///   - 正断言：事件流包含 Queued，说明 tool_call 已进入 card task queue
///   - 正断言：当前轮不会额外发送 ToolResult，因为有效 enqueue 先只做入队
///   - 负断言：有效 enqueue 不应把 shell 置为 Realtime error
#[gpui::test]
async fn test_realtime_tool_call_enqueue_routes_to_card_task_queue(cx: &mut TestAppContext) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (events, tool_results, label) = app.update_in(cx, |app, _window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_tool_enqueue"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar should accept start_session");
            let _ = app.drain_realtime_sidecar(cx);

            sidecar.push_event(Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: realtime::RealtimeSessionId::new("rt_tool_enqueue"),
                tool_call_id: "call_enqueue_1".to_string(),
                name: "enqueue_card_task".to_string(),
                arguments: json!({ "intent": "创建 Codex 卡片" }),
            })));
            let _ = app.drain_realtime_sidecar(cx);

            let commands = sidecar.commands();
            let tool_results = commands
                .into_iter()
                .filter_map(|command| match command {
                    RealtimeCommand::ToolResult(result) => Some(result),
                    _ => None,
                })
                .collect::<Vec<_>>();
            (
                app.realtime.drain_card_task_events(),
                tool_results,
                app.effective_voice_label(),
            )
        });

        assert!(
            events
                .iter()
                .any(|event| matches!(event, CardTaskEvent::Queued { .. })),
            "valid enqueue_card_task tool_call should enqueue a card task and emit Queued"
        );
        assert!(
            tool_results.is_empty(),
            "valid enqueue_card_task tool_call should not immediately emit ToolResult before Task 5 callback wiring"
        );
        assert_ne!(
            label, "Realtime error",
            "valid enqueue_card_task tool_call must not push the shell into Realtime error"
        );
    });
}

/// RTC-2：sidecar `cancel_card_task` tool_call 应路由到 cancel entrypoint，并撤销最新排队任务。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id    = "rt_tool_cancel"
///   tool_call_id  = "call_cancel_1"
///   预先 direct enqueue 1 个 task，使 cancel 路由后必然有可取消对象
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 RecordingSidecar realtime shell，并先 drain 掉 session_started
///   2. 直接调用 enqueue_realtime_card_task(...) 造出一个 queued task
///   3. 注入一条 ToolCall(cancel_card_task)，再调用 app drain sidecar 入口
///   4. 读取 card-task 事件流和 sidecar ToolResult commands
///
/// 预期结果：
///   - 正断言：事件流包含 Cancelled，说明 cancel tool_call 已命中 app cancel entrypoint
///   - 正断言：cancel tool_call 自己的 tool_call_id 收到 status=cancelled 的 ToolResult
///   - 负断言：cancel 路由不应产生 Realtime error
#[gpui::test]
async fn test_realtime_tool_call_cancel_routes_to_cancel_entrypoint(cx: &mut TestAppContext) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (events, tool_results, label) = app.update_in(cx, |app, _window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_tool_cancel"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar should accept start_session");
            let _ = app.drain_realtime_sidecar(cx);

            app.enqueue_realtime_card_task(
                "先排一个任务".to_string(),
                "call_direct_enqueue".to_string(),
                cx,
            );
            let _ = app.realtime.drain_card_task_events();

            sidecar.push_event(Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: realtime::RealtimeSessionId::new("rt_tool_cancel"),
                tool_call_id: "call_cancel_1".to_string(),
                name: "cancel_card_task".to_string(),
                arguments: json!({ "scope": "latest" }),
            })));
            let _ = app.drain_realtime_sidecar(cx);

            let tool_results = sidecar
                .commands()
                .into_iter()
                .filter_map(|command| match command {
                    RealtimeCommand::ToolResult(result) => Some(result),
                    _ => None,
                })
                .collect::<Vec<RealtimeToolResult>>();

            (
                app.realtime.drain_card_task_events(),
                tool_results,
                app.effective_voice_label(),
            )
        });

        assert!(
            events
                .iter()
                .any(|event| matches!(event, CardTaskEvent::Cancelled { .. })),
            "cancel_card_task tool_call should route to cancel_realtime_card_task and emit Cancelled"
        );
        assert_ne!(
            label, "Realtime error",
            "cancel_card_task routing must not push the shell into Realtime error"
        );
        assert!(
            tool_results
                .iter()
                .any(|result| result.tool_call_id == "call_cancel_1"
                    && result.result["status"] == json!("cancelled")),
            "cancel_card_task tool_call 自己必须收到 status=cancelled 的 ToolResult"
        );
    });
}

/// RTC-2b：stale session 的 tool_call 不应进入当前 card task queue，并应返回失败 ToolResult。
///
/// 数据构造（含关键数值的推导过程）：
///   active session_id = "rt_tool_current"
///   stale session_id  = "rt_tool_stale"
///   tool_call_id      = "call_stale_enqueue"
///   初始 queue 为空，因此正确拒绝 stale call 后事件流仍为空
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 RecordingSidecar 启动 active session，并 drain 到 Listening
///   2. 注入一条 session_id 不匹配的 enqueue_card_task tool_call
///   3. 调用 app drain sidecar 入口
///   4. 读取 card-task events 与 sidecar ToolResult commands
///
/// 预期结果：
///   - 正断言：stale call 收到 status=failed 的 ToolResult
///   - 正断言：失败摘要说明 session mismatch / stale session
///   - 负断言：stale call 不应创建 Queued 事件
#[gpui::test]
async fn test_realtime_tool_call_from_stale_session_is_rejected(cx: &mut TestAppContext) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (events, tool_results) = app.update_in(cx, |app, _window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_tool_current"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar should accept start_session");
            let _ = app.drain_realtime_sidecar(cx);

            sidecar.push_event(Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: realtime::RealtimeSessionId::new("rt_tool_stale"),
                tool_call_id: "call_stale_enqueue".to_string(),
                name: "enqueue_card_task".to_string(),
                arguments: json!({ "intent": "创建 Codex 卡片" }),
            })));
            let _ = app.drain_realtime_sidecar(cx);

            let tool_results = sidecar
                .commands()
                .into_iter()
                .filter_map(|command| match command {
                    RealtimeCommand::ToolResult(result) => Some(result),
                    _ => None,
                })
                .collect::<Vec<RealtimeToolResult>>();

            (app.realtime.drain_card_task_events(), tool_results)
        });

        assert!(
            events.is_empty(),
            "stale session 的 tool_call 不应进入当前 card task queue"
        );
        assert!(
            tool_results
                .iter()
                .any(|result| result.tool_call_id == "call_stale_enqueue"
                    && result.result["status"] == json!("failed")),
            "stale enqueue tool_call 必须收到 status=failed 的 ToolResult"
        );
        assert!(
            tool_results.iter().any(|result| result.result["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("session"))),
            "stale enqueue 失败摘要应说明 session 不匹配，方便定位过期 tool_call"
        );
    });
}

/// RTC-3：unknown tool name 与缺少 intent 的 enqueue tool_call 都应走失败 ToolResult 基础路径。
///
/// 数据构造（含关键数值的推导过程）：
///   case A tool_call_id = "call_missing_intent"，name = enqueue_card_task，arguments = {}
///   case B tool_call_id = "call_unknown_tool"，name = rename_card_task，arguments = {"intent":"x"}
///   两个 case 都没有合法 task 可入队，因此预期输出 2 条 failed ToolResult
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 RecordingSidecar realtime shell，并先 drain 掉 session_started
///   2. 顺序注入 missing-intent 与 unknown-tool 两条 ToolCall
///   3. 每次调用 app drain sidecar 入口，让路由逻辑把失败结果发回 sidecar command 通道
///   4. 读取 RecordingSidecar 中的 ToolResult commands
///
/// 预期结果：
///   - 正断言：两条 ToolResult 都是 status=failed
///   - 正断言：summary 分别说明 missing intent 与 unknown tool
///   - 负断言：非法 tool_call 不应偷偷入队 card task
#[gpui::test]
async fn test_realtime_tool_call_invalid_cases_emit_failed_tool_results(cx: &mut TestAppContext) {
    with_realtime_process_guard(|| {
        let sidecar = RecordingSidecar::default();
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (events, tool_results) = app.update_in(cx, |app, _window, cx| {
            app.realtime
                .start_shell_with_sidecar_for_tests(
                    config("rt_tool_invalid"),
                    Box::new(sidecar.clone()),
                )
                .expect("recording sidecar should accept start_session");
            let _ = app.drain_realtime_sidecar(cx);

            sidecar.push_event(Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: realtime::RealtimeSessionId::new("rt_tool_invalid"),
                tool_call_id: "call_missing_intent".to_string(),
                name: "enqueue_card_task".to_string(),
                arguments: json!({}),
            })));
            let _ = app.drain_realtime_sidecar(cx);

            sidecar.push_event(Ok(RealtimeEvent::ToolCall(RealtimeToolCall {
                session_id: realtime::RealtimeSessionId::new("rt_tool_invalid"),
                tool_call_id: "call_unknown_tool".to_string(),
                name: "rename_card_task".to_string(),
                arguments: json!({ "intent": "x" }),
            })));
            let _ = app.drain_realtime_sidecar(cx);

            let tool_results = sidecar
                .commands()
                .into_iter()
                .filter_map(|command| match command {
                    RealtimeCommand::ToolResult(result) => Some(result),
                    _ => None,
                })
                .collect::<Vec<RealtimeToolResult>>();
            (app.realtime.drain_card_task_events(), tool_results)
        });

        assert!(
            events.is_empty(),
            "invalid realtime tool_call cases should not enqueue any card task events"
        );
        assert_eq!(
            tool_results.len(),
            2,
            "missing intent and unknown tool should each emit one failed ToolResult"
        );
        assert_eq!(
            tool_results[0].result["status"],
            json!("failed"),
            "missing-intent realtime tool_call should emit status=failed"
        );
        assert!(
            tool_results[0].result["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("intent")),
            "missing-intent realtime tool_call should explain the missing intent in summary"
        );
        assert_eq!(
            tool_results[1].result["status"],
            json!("failed"),
            "unknown-tool realtime tool_call should emit status=failed"
        );
        assert!(
            tool_results[1].result["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("unknown tool")),
            "unknown-tool realtime tool_call should explain the unknown tool name in summary"
        );
    });
}
