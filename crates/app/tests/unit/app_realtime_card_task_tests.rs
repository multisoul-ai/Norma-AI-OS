use super::card_tasks::{
    build_realtime_card_task_planning_request, AppCardTaskRunner, RealtimeCardTaskState,
    DEFAULT_MAX_CARD_TASK_STEPS,
};
use crate::app::MultisoulApp;
use crate::canvas::master_layout::MasterLayoutState;
use crate::motion::MotionStore;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, TermSpec};
use card_task::{
    CardCommandRunner, CardTaskEvent, CardTaskPlanJsonProvider, CardTaskPlanner,
    CardTaskPlannerError, CardTaskRequest,
};
use gpui::TestAppContext;
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult, CardKind, Placement};
use realtime::{RealtimeSessionConfig, RealtimeSessionId};
use runtime::Runtime;
use std::collections::VecDeque;
use voice::{CompiledVoicePlan, ExecutableCommand, RiskLevel, VoicePlanningRequest};

fn config(session_id: &str) -> RealtimeSessionConfig {
    RealtimeSessionConfig {
        session_id: RealtimeSessionId::new(session_id),
        model: "gpt-realtime-2".to_string(),
        tools: Vec::new(),
    }
}

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
struct FakeJsonPlanner {
    requests: Vec<VoicePlanningRequest>,
    results: VecDeque<Result<String, CardTaskPlannerError>>,
}

impl FakeJsonPlanner {
    fn with_results(results: Vec<Result<String, CardTaskPlannerError>>) -> Self {
        Self {
            requests: Vec::new(),
            results: results.into(),
        }
    }
}

impl CardTaskPlanJsonProvider for FakeJsonPlanner {
    fn plan_json(
        &mut self,
        request: &VoicePlanningRequest,
    ) -> Result<String, CardTaskPlannerError> {
        self.requests.push(request.clone());
        self.results
            .pop_front()
            .expect("fake json planner 应预先准备结果")
    }
}

#[derive(Default)]
struct FakeRunner {
    commands: Vec<CardCommand>,
    results: VecDeque<Result<CardCommandResult, CardCommandError>>,
}

impl FakeRunner {
    fn with_results(results: Vec<Result<CardCommandResult, CardCommandError>>) -> Self {
        Self {
            commands: Vec::new(),
            results: results.into(),
        }
    }
}

impl CardCommandRunner for FakeRunner {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        self.commands.push(command);
        self.results
            .pop_front()
            .unwrap_or_else(|| Ok(CardCommandResult::Listed { cards: Vec::new() }))
    }
}

fn compiled_plan(commands: Vec<ExecutableCommand>) -> CompiledVoicePlan {
    CompiledVoicePlan {
        commands,
        requires_confirmation: false,
        risk: RiskLevel::Low,
        reason: Some("test".to_string()),
    }
}

/// ARC-1：`MultisoulApp` 的 realtime card-task entrypoint 不应打断 Listening shell，且 cancel 应撤销最新排队任务。
///
/// 数据构造（含关键数值的推导过程）：
///   realtime session_id = rt-card-enqueue
///   planner snapshot max_steps = DEFAULT_MAX_CARD_TASK_STEPS = 8；第 1 个 task = create terminal（执行完成后 document.nodes = 1）
///   第 2 个 task = queued only，随后立刻 cancel_latest
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 GPUI test window 创建 MultisoulApp，并让 realtime shell 进入 Listening
///   2. 注入 fake compiled planner，然后调用 enqueue_realtime_card_task(...)
///   3. 连续 drain_realtime_card_tasks(...) 两次，完成 planning + create terminal
///   4. 再 enqueue 第二个 task，随后调用 cancel_realtime_card_task(...)，最后 drain_events 读取 Queued / Succeeded / Cancelled 事件
///
/// 预期结果：
///   - 正断言：enqueue / execute / cancel 全程底栏仍为 Always Listening；第一条 task 真正创建 terminal，第二条 task 产出 Cancelled
///   - 负断言：entrypoint 不应把 shell 退回 Connecting / Realtime error
#[gpui::test]
async fn test_realtime_card_task_app_entrypoints_preserve_shell_and_cancel_latest_task(
    cx: &mut TestAppContext,
) {
    super::app_realtime_tests_chat_support::with_realtime_process_guard(|| {
        let (app, cx) = cx.add_window_view(|_, cx| MultisoulApp::new(cx));
        let (label, node_exists, events) = app.update_in(cx, |app, window, cx| {
            app.realtime
                .start_shell_with_config(config("rt-card-enqueue"))
                .expect("有效 realtime config 应能进入 Connecting");
            app.realtime.mark_started();
            app.realtime
                .card_tasks_for_tests()
                .set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(vec![
                    Ok(compiled_plan(vec![ExecutableCommand::Card(
                        CardCommand::Create {
                            kind: CardKind::Terminal { shell: None },
                            placement: Placement::VisibleArea,
                        },
                    )])),
                ])));

            app.enqueue_realtime_card_task(
                "创建一个 Terminal 卡片".to_string(),
                "call-enqueue".to_string(),
                cx,
            );
            let _ = app.drain_realtime_card_tasks(window, cx);
            let _ = app.drain_realtime_card_tasks(window, cx);

            app.enqueue_realtime_card_task(
                "取消这个任务".to_string(),
                "call-cancel".to_string(),
                cx,
            );
            app.cancel_realtime_card_task(cx);

            (
                app.effective_voice_label(),
                app.runtime.document.node(1).is_some(),
                app.realtime.drain_card_task_events(),
            )
        });

        assert_eq!(
            label, "Always Listening",
            "enqueue/cancel realtime card task 不应打断已处于 Listening 的 chat shell"
        );
        assert!(
            node_exists,
            "第一条 realtime card task 经 app drain 后应真正创建 terminal 节点"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, CardTaskEvent::Queued { .. })),
            "app enqueue_realtime_card_task(...) 后事件流必须包含 Queued"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, CardTaskEvent::Succeeded { .. })),
            "第一条 realtime card task 执行完成后事件流必须包含 Succeeded"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, CardTaskEvent::Cancelled { .. })),
            "cancel_realtime_card_task(...) 必须把最新排队任务转成 Cancelled 事件"
        );
        assert_ne!(
            label, "Connecting...",
            "entrypoint 执行期间 shell 不应退回 Connecting"
        );
        assert_ne!(
            label, "Realtime error",
            "单纯 enqueue/cancel realtime card task 不应把 shell 置为错误态"
        );
    });
}

/// ARC-2：planning request 应从当前 runtime 快照构造 transcript/cards/focus/selection/max_steps。
///
/// 数据构造（含关键数值的推导过程）：
///   node 1 = Terminal
///   node 2 = Codex Agent(alias=alan)
///   selected ids = [1]
///   focused id = 2
///   max_steps = 8
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Runtime 中放入 Terminal + Codex Agent
///   2. selection.select_only(1)；focus.focus_node(2)
///   3. build_realtime_card_task_planning_request(runtime, intent, 8)
///
/// 预期结果：
///   - 正断言：transcript=intent，cards 含两张卡
///   - 正断言：focused/selected/max_steps 被完整拷贝
///   - 负断言：Agent alias 不应丢失
#[test]
fn test_build_realtime_card_task_planning_request_uses_runtime_snapshot() {
    let mut runtime = Runtime::new();
    let terminal_id = runtime.document.add_node(
        CanvasNodeKind::Term(TermSpec),
        CanvasRect::new(0.0, 0.0, 320.0, 180.0),
    );
    let agent_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: Some("alan".to_string()),
        }),
        CanvasRect::new(360.0, 0.0, 320.0, 180.0),
    );
    runtime.selection.select_only(terminal_id);
    runtime.focus.focus_node(agent_id);

    let request = build_realtime_card_task_planning_request(
        &runtime,
        "帮我整理卡片".to_string(),
        DEFAULT_MAX_CARD_TASK_STEPS,
    );

    assert_eq!(
        request.transcript, "帮我整理卡片",
        "planning request transcript 应直接使用 realtime card task intent"
    );
    assert_eq!(
        request.cards.len(),
        2,
        "planning request 应包含当前 runtime 的全部卡片摘要"
    );
    assert_eq!(
        request.focused_node_id,
        Some(agent_id),
        "planning request 应记录当前 focused card id"
    );
    assert_eq!(
        request.selected_node_ids,
        vec![terminal_id],
        "planning request 应记录当前 selected card ids"
    );
    assert_eq!(
        request.max_steps, DEFAULT_MAX_CARD_TASK_STEPS,
        "planning request 应保留 realtime card task 的默认最大步数"
    );
    assert_eq!(
        request
            .cards
            .iter()
            .find(|card| card.node_id == agent_id)
            .and_then(|card| card.alias.clone()),
        Some("alan".to_string()),
        "Agent alias 应进入 planning request，供 planner 做目标解析"
    );
    assert_ne!(
        request.focused_node_id,
        Some(terminal_id),
        "focused id 不应误用 selected terminal id"
    );
}

/// ARC-3：host 应按串行顺序执行 compiled Create/List/Focus/Delete 命令。
///
/// 数据构造（含关键数值的推导过程）：
///   planner results = 1 个四步 compiled plan
///   runner results = Created(41) + Listed + Focused(41) + Deleted(41)
///   drain 次数 = 4 步执行 + 1 次收尾 succeeded = 5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 配置 fake compiled planner 与 fake runner
///   2. enqueue_task(...)
///   3. 连续 run_next 五次，驱动 planning/执行/收尾
///   4. 读取 runner.commands 与事件流
///
/// 预期结果：
///   - 正断言：runner 收到的命令顺序精确为 Create/List/Focus/Delete
///   - 正断言：事件流最终包含 Succeeded
///   - 负断言：Delete 不应早于 Focus 执行
#[test]
fn test_realtime_card_task_host_executes_compiled_commands_in_serial_order() {
    let mut host = RealtimeCardTaskState::default();
    host.set_compiled_planner_for_tests(Box::new(FakeCompiledPlanner::with_results(vec![Ok(
        compiled_plan(vec![
            ExecutableCommand::Card(CardCommand::Create {
                kind: CardKind::Terminal { shell: None },
                placement: Placement::VisibleArea,
            }),
            ExecutableCommand::Card(CardCommand::List),
            ExecutableCommand::Card(CardCommand::Focus { node_id: 41 }),
            ExecutableCommand::Card(CardCommand::Delete { node_id: 41 }),
        ]),
    )])));
    let request = build_realtime_card_task_planning_request(
        &Runtime::new(),
        "创建后再删除".to_string(),
        DEFAULT_MAX_CARD_TASK_STEPS,
    );
    host.enqueue_task(
        "创建后再删除".to_string(),
        "call-serial".to_string(),
        request,
    );
    let mut runner = FakeRunner::with_results(vec![
        Ok(CardCommandResult::Created { node_id: 41 }),
        Ok(CardCommandResult::Listed { cards: Vec::new() }),
        Ok(CardCommandResult::Focused { node_id: 41 }),
        Ok(CardCommandResult::Deleted { node_id: 41 }),
    ]);

    for _ in 0..5 {
        let _ = host.run_next(&mut runner);
    }
    let events = host.drain_events();

    assert_eq!(
        runner.commands,
        vec![
            CardCommand::Create {
                kind: CardKind::Terminal { shell: None },
                placement: Placement::VisibleArea,
            },
            CardCommand::List,
            CardCommand::Focus { node_id: 41 },
            CardCommand::Delete { node_id: 41 },
        ],
        "host 应按 compiled plan 的串行顺序执行 Create/List/Focus/Delete"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CardTaskEvent::Succeeded { .. })),
        "四步命令执行完成后事件流必须包含 Succeeded"
    );
    assert!(
        runner
            .commands
            .iter()
            .position(|command| matches!(command, CardCommand::Delete { .. }))
            > runner
                .commands
                .iter()
                .position(|command| matches!(command, CardCommand::Focus { .. })),
        "Delete 不应早于 Focus 执行，否则 host 没有保持串行顺序"
    );
}

/// ARC-4：planner provider error 与 malformed planner JSON 都应转成 Failed 事件。
///
/// 数据构造（含关键数值的推导过程）：
///   case A provider error = "planner down"
///   case B planner json = "{bad"
///   两个 case 各 enqueue 1 个 task，runner 不需要执行任何命令
///
/// 执行过程（逐步说明系统如何处理）：
///   1. case A 用 provider error planner 执行一次 run_next
///   2. case B 用 malformed json planner 执行一次 run_next
///   3. 分别读取 Failed 事件消息
///
/// 预期结果：
///   - 正断言：两种失败都产出 Failed 事件
///   - 负断言：失败场景不应继续执行 runner 命令
#[test]
fn test_realtime_card_task_host_turns_provider_and_json_failures_into_failed_events() {
    let request = build_realtime_card_task_planning_request(
        &Runtime::new(),
        "失败用例".to_string(),
        DEFAULT_MAX_CARD_TASK_STEPS,
    );

    let mut provider_host = RealtimeCardTaskState::default();
    provider_host.set_json_planner_for_tests(Box::new(FakeJsonPlanner::with_results(vec![Err(
        CardTaskPlannerError::Provider {
            message: "planner down".to_string(),
        },
    )])));
    provider_host.enqueue_task(
        "provider failed".to_string(),
        "call-provider".to_string(),
        request.clone(),
    );
    let mut provider_runner = FakeRunner::default();
    let _ = provider_host.run_next(&mut provider_runner);
    let provider_events = provider_host.drain_events();

    let mut json_host = RealtimeCardTaskState::default();
    json_host.set_json_planner_for_tests(Box::new(FakeJsonPlanner::with_results(vec![Ok(
        "{bad".to_string()
    )])));
    json_host.enqueue_task("json failed".to_string(), "call-json".to_string(), request);
    let mut json_runner = FakeRunner::default();
    let _ = json_host.run_next(&mut json_runner);
    let json_events = json_host.drain_events();

    assert!(
        provider_events.iter().any(|event| matches!(event, CardTaskEvent::Failed { message, .. } if message.contains("planner down"))),
        "provider error 必须转成 Failed(planner down) 事件"
    );
    assert!(
        json_events.iter().any(|event| matches!(event, CardTaskEvent::Failed { message, .. } if message.contains("json"))),
        "malformed planner JSON 必须转成 Failed(json ...) 事件"
    );
    assert!(
        provider_runner.commands.is_empty(),
        "planner provider error 时不应继续执行任何 runner 命令"
    );
    assert!(
        json_runner.commands.is_empty(),
        "planner JSON 解析失败时不应继续执行任何 runner 命令"
    );
}

/// ARC-5：AppCardTaskRunner 应复用 execute_card_command_for_area，并汇总 canvas/session dirty effects。
///
/// 数据构造（含关键数值的推导过程）：
///   area = 1200x900
///   zoom = 1.0
///   command 1 = Create Terminal → 预期 canvas_dirty=true, session_dirty=true
///   command 2 = List → 预期不再新增 dirty，但会返回一张卡片摘要
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 Runtime/Motion/MasterLayout
///   2. AppCardTaskRunner::run(Create Terminal)
///   3. AppCardTaskRunner::run(List)
///   4. 读取汇总 effects 与 runtime 文档
///
/// 预期结果：
///   - 正断言：Create 创建 live terminal，effects 汇总含 canvas/session dirty
///   - 正断言：List 返回一张卡片摘要
///   - 负断言：runner 不应丢失 Create 带来的 dirty flags
#[test]
fn test_app_card_task_runner_reuses_norma_executor_and_accumulates_effects() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut master_layout = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1200.0, 900.0);
    let zoom = 1.0;
    let mut runner =
        AppCardTaskRunner::new(&mut runtime, &mut motion, &mut master_layout, area, zoom);

    let created = runner
        .run(CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        })
        .expect("AppCardTaskRunner 应能执行 create terminal");
    let listed = runner
        .run(CardCommand::List)
        .expect("AppCardTaskRunner 应能执行 list");
    let effects = runner.finish();

    assert_eq!(
        created,
        CardCommandResult::Created { node_id: 1 },
        "AppCardTaskRunner 首次 create terminal 应创建 node_id=1"
    );
    assert!(
        runtime.document.node(1).is_some(),
        "AppCardTaskRunner 复用 Norma executor 后 document 应出现 node_id=1"
    );
    assert!(
        matches!(listed, CardCommandResult::Listed { ref cards } if cards.len() == 1),
        "创建后立即 list 应返回一张卡片摘要"
    );
    assert!(
        effects.canvas_dirty,
        "Create Terminal 带来的 canvas_dirty 不应在后续 List 后丢失"
    );
    assert!(
        effects.session_dirty,
        "Create Terminal 带来的 session_dirty 不应在后续 List 后丢失"
    );
}
