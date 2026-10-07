use super::*;
use crate::settings::voice::{
    DesktopVoiceMode, EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState,
};
use canvas::{AgentProfile, AgentSpec, CanvasRect};
use gpui::TestAppContext;
use runtime::AgentTaskUpdate;

fn configure_task(
    app: &mut MultisoulApp,
) -> (
    NodeId,
    async_channel::Receiver<crate::app::doubao::DoubaoReport>,
) {
    app.voice_settings = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            desktop_mode: Some(DesktopVoiceMode::Doubao),
            doubao_credential_ref: Some("synthetic-doubao-account".into()),
            ..Default::default()
        },
        EnvVoiceSettings::default(),
    );
    let (sender, reports) = async_channel::unbounded();
    app.doubao.reports = Some(sender);
    let node_id = app.runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: Some("Chloe".into()),
            definition_id: Some(1),
            mobile_control: false,
        }),
        CanvasRect::new(0.0, 0.0, 640.0, 360.0),
    );
    app.agent_task_coordinator = Some(
        agent::AgentTaskCoordinator::new(vec![agent::AgentRosterEntry {
            definition_id: 1,
            name: "Chloe".into(),
            speech_alias: None,
            profile: norma::protocol::AgentProfile::Claude,
            enabled: true,
            active_node_id: Some(node_id),
            tracking: agent::AgentTrackingMode::Managed,
            busy: false,
            task_state: agent::AgentTaskState::Ready,
        }])
        .expect("应创建真实任务协调器"),
    );
    (node_id, reports)
}

fn update_task(app: &mut MultisoulApp, node_id: NodeId, state: AgentTaskState, event: &str) {
    app.runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Claude,
        session_id: None,
        to: state,
        provider_event: event.into(),
        task_title: None,
    });
}

/// 取消后迟到的 Stop：豆包播报必须服从协调器的中断终态，不能把原始 Done 当作成功。
///
/// 数据构造（含关键数值的推导过程）：
///   Chloe 卡片 1 张，定义 id=1；真实协调器派出 1 个任务，再对同一 task_id 请求取消。
///   卡片矩形 640×360 仅提供合法几何；本地输出通道替代网络，不启动 PTY 或麦克风。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 派出任务并打开 UserPromptSubmit 回合，确认没有终态播报。
///   2. 请求取消，随后注入迟到的 Stop/Done；先处理真实 RuntimeEvent，覆盖无 Hook 推进路径。
///   3. 再通过生产队列推进确认 interrupted，读取豆包输出并再次处理空事件批次。
///
/// 预期结果：
///   - 正断言：协调器终态为 interrupted，豆包输出为 Chloe, Interrupted。
///   - 负断言：不能播报 Done，也不能在下一轮重复播报同一个终态。
#[test]
fn doubao_cancelled_task_does_not_announce_late_done() {
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let reports = app.update(cx, |app, cx| {
        let (node_id, reports) = configure_task(app);
        let coordinator = app.agent_task_coordinator.as_mut().expect("应有协调器");
        let receipt = coordinator
            .dispatch(
                vec![agent::AgentAssignment {
                    target: "Chloe".into(),
                    task: "Inspect cancellation".into(),
                }],
                false,
            )
            .expect("应派出任务")
            .remove(0);
        update_task(app, node_id, AgentTaskState::Thinking, "UserPromptSubmit");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        assert!(reports.try_recv().is_err(), "开始任务不能生成终态播报");
        app.agent_task_coordinator
            .as_mut()
            .expect("应有协调器")
            .cancel_task(receipt.task_id);
        update_task(app, node_id, AgentTaskState::Done, "Stop");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        app.advance_agent_task_queue(node_id);
        let task = app
            .agent_task_coordinator
            .as_ref()
            .expect("应有协调器")
            .tasks()
            .remove(0);
        assert_eq!(
            task.state, "interrupted",
            "迟到 Done 不能改变已请求取消的任务结果"
        );
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        let report = reports.try_recv().expect("真实中断结果必须进入豆包输出");
        assert_eq!(
            report.text, "Chloe, Interrupted",
            "播报必须使用协调器确认的中断终态"
        );
        assert_ne!(report.text, "Chloe, Done", "已取消任务不能播报完成");
        report.completed.send(()).expect("应确认播报完成");
        reports
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| app.handle_agent_runtime_events(&[], cx));
    assert!(reports.try_recv().is_err(), "终态下一轮不能重复播报");
}

/// 手动关闭 Agent：没有 Stop 的 SessionEnd 也必须播报协调器确认的中断结果。
///
/// 数据构造：Chloe 卡片 1 张、真实运行任务 1 个，本地汇报通道代替网络；
///   卡片 640×360 提供合法几何，不创建 PTY 或麦克风。
///
/// 执行过程：
///   1. 派出任务，注入 UserPromptSubmit 开始回合。
///   2. 注入 SessionEnd/Ended，并通过生产队列推进收口任务。
///   3. 处理真实 RuntimeEvent，确认播报并推进下一帧。
///
/// 预期结果：
///   - 正断言：任务为 interrupted，播报 Chloe, Interrupted。
///   - 负断言：不播报 Done，不重复播报中断。
#[test]
fn doubao_session_end_announces_interrupted_task() {
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let reports = app.update(cx, |app, cx| {
        let (node_id, reports) = configure_task(app);
        app.agent_task_coordinator
            .as_mut()
            .expect("应有协调器")
            .dispatch(
                vec![agent::AgentAssignment {
                    target: "Chloe".into(),
                    task: "Inspect termination".into(),
                }],
                false,
            )
            .expect("应派出任务");
        update_task(app, node_id, AgentTaskState::Thinking, "UserPromptSubmit");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        update_task(app, node_id, AgentTaskState::Ended, "SessionEnd");
        app.advance_agent_task_queue(node_id);
        let task = app
            .agent_task_coordinator
            .as_ref()
            .expect("应有协调器")
            .tasks()
            .remove(0);
        assert_eq!(task.state, "interrupted", "关闭 Agent 必须收口运行任务");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        let report = reports.try_recv().expect("SessionEnd 不能漏播中断结果");
        assert_eq!(report.text, "Chloe, Interrupted", "关闭 Agent 应播报中断");
        assert_ne!(report.text, "Chloe, Done", "关闭 Agent 不能播报完成");
        report.completed.send(()).expect("应确认播报完成");
        reports
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| app.handle_agent_runtime_events(&[], cx));
    assert!(reports.try_recv().is_err(), "SessionEnd 结果不能重复播报");
}

/// 取消排队任务：应独立播报取消，不得占用同卡片运行任务的完成播报。
///
/// 数据构造：Chloe 卡片 1 张；任务 A 运行、任务 B 排队，共 2 个 task_id；
///   本地汇报通道代替网络，卡片 640×360 提供合法几何。
///
/// 执行过程：
///   1. 派出 A 并打开回合，再排队 B。
///   2. 取消 B，空事件帧必须播报 Cancelled；确认播放完成。
///   3. 注入 A 的 Stop/Done，通过生产队列推进，再处理真实 RuntimeEvent。
///
/// 预期结果：
///   - 正断言：B 为 cancelled，A 仍 running；随后 A 成功并播报 Done。
///   - 负断言：A 不能被 B 的取消误判或抑制，下一帧不能重复播报。
#[test]
fn doubao_queued_cancellation_preserves_active_task_completion() {
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let (node_id, active_id, reports) = app.update(cx, |app, cx| {
        let (node_id, reports) = configure_task(app);
        let coordinator = app.agent_task_coordinator.as_mut().expect("应有协调器");
        let active = coordinator
            .dispatch(
                vec![agent::AgentAssignment {
                    target: "Chloe".into(),
                    task: "Active task".into(),
                }],
                false,
            )
            .expect("应派出 A")
            .remove(0);
        let queued = coordinator
            .dispatch(
                vec![agent::AgentAssignment {
                    target: "Chloe".into(),
                    task: "Queued task".into(),
                }],
                true,
            )
            .expect("应排队 B")
            .remove(0);
        update_task(app, node_id, AgentTaskState::Thinking, "UserPromptSubmit");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        let coordinator = app.agent_task_coordinator.as_mut().expect("应有协调器");
        assert_eq!(
            coordinator.cancel_task(queued.task_id),
            agent::AgentTaskCancellation::Cancelled,
            "尚未提交的 B 应直接取消"
        );
        let tasks = coordinator.tasks();
        assert_eq!(
            tasks
                .iter()
                .find(|task| task.task_id == active.task_id)
                .expect("应保留 A")
                .state,
            "running",
            "取消 B 不能中断 A"
        );
        assert_eq!(
            tasks
                .iter()
                .find(|task| task.task_id == queued.task_id)
                .expect("应保留 B 终态")
                .state,
            "cancelled",
            "B 必须确认取消"
        );
        app.handle_agent_runtime_events(&[], cx);
        let report = reports.try_recv().expect("无卡片终态事件也必须播报 B 取消");
        assert_eq!(report.text, "Chloe, Cancelled", "排队取消不能误报完成");
        report.completed.send(()).expect("应确认 B 播报完成");
        (node_id, active.task_id, reports)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        update_task(app, node_id, AgentTaskState::Done, "Stop");
        app.advance_agent_task_queue(node_id);
        let tasks = app
            .agent_task_coordinator
            .as_ref()
            .expect("应有协调器")
            .tasks();
        assert_eq!(
            tasks
                .iter()
                .find(|task| task.task_id == active_id)
                .expect("应保留 A 终态")
                .state,
            "succeeded",
            "A 必须独立成功"
        );
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        let report = reports.try_recv().expect("B 的取消不能抑制 A 的完成播报");
        assert_eq!(report.text, "Chloe, Done", "A 必须播报真实成功");
        assert_ne!(report.text, "Chloe, Cancelled", "A 不能沿用 B 的取消结果");
        report.completed.send(()).expect("应确认 A 播报完成");
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| app.handle_agent_runtime_events(&[], cx));
    assert!(reports.try_recv().is_err(), "已播报的 A 和 B 不得重复");
}

/// 原生卡片完成：turn/completed 没有伴随 Hook 队列推进时，仍须播报一次真实结果。
///
/// 数据构造：真实协调器派出 1 个任务，卡片边界发出 turn/started、turn/completed；
///   协调器暂时仍为 running，本地汇报通道代替网络。
///
/// 执行过程：
///   1. 派工后打开原生回合，再处理真实完成事件；故意不手动推进协调器。
///   2. 确认播报完成，再让协调器追上 succeeded 并处理空事件帧。
///
/// 预期结果：
///   - 正断言：卡片终态立即播报 Chloe, Done。
///   - 负断言：协调器迟到的同任务终态不能再次播报。
#[test]
fn doubao_native_completion_does_not_require_hook_queue_transition() {
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let reports = app.update(cx, |app, cx| {
        let (node_id, reports) = configure_task(app);
        app.agent_task_coordinator
            .as_mut()
            .expect("应有协调器")
            .dispatch(
                vec![agent::AgentAssignment {
                    target: "Chloe".into(),
                    task: "Native completion".into(),
                }],
                false,
            )
            .expect("应派出任务");
        update_task(app, node_id, AgentTaskState::Thinking, "turn/started");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        update_task(app, node_id, AgentTaskState::Done, "turn/completed");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        let report = reports
            .try_recv()
            .expect("原生完成事件不能等待不存在的 Hook 推进");
        assert_eq!(report.text, "Chloe, Done", "原生卡片完成必须播报成功");
        report.completed.send(()).expect("应确认播报完成");
        app.agent_task_coordinator
            .as_mut()
            .expect("应有协调器")
            .finish_current(1);
        reports
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| app.handle_agent_runtime_events(&[], cx));
    assert!(
        reports.try_recv().is_err(),
        "协调器追上同一 task_id 后不能重复播报"
    );
}

/// 旧中断与新续聊同批到达：工具中间态不能让旧任务占用新回合的完成名额。
///
/// 数据构造：协调任务 1 个，先被中断；随后无 task_id 的续聊回合 1 个，
///   同批事件为 Ended → turn/started → item/started，共 3 个边界/中间态事件。
///
/// 执行过程：
///   1. 中断旧任务并收口协调器，在处理事件前启动新续聊及工具调用。
///   2. 处理整批事件，确认旧 Interrupted 播报已完成。
///   3. 完成新续聊，检查独立 Done 播报。
///
/// 预期结果：
///   - 正断言：旧 Interrupted 和新 Done 各播报一次。
///   - 负断言：新回合不能被工具中间态或旧终态抢占，不重复播报旧 Interrupted。
#[test]
fn doubao_interrupted_task_does_not_claim_new_reply_round() {
    assert_interrupted_task_then_reply(false);
}

/// 同批跨回合：旧中断与新续聊完成必须保留两次独立播报。
///
/// 数据构造：1 个协调任务与 1 个无 task_id 续聊；同批 Ended → started → tool → Done。
/// 执行过程：收口旧中断，注入完整新回合后一次处理事件；确认旧播报完成并推进播放队列。
/// 预期结果：旧 Interrupted 和新 Done 各存在，不能合并为一次中断或漏掉新成功。
#[test]
fn doubao_same_batch_interruption_and_reply_completion_are_distinct() {
    assert_interrupted_task_then_reply(true);
}

fn assert_interrupted_task_then_reply(finish_in_batch: bool) {
    let mut test = TestAppContext::single();
    let (app, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    let (node_id, reports) = app.update(cx, |app, cx| {
        let (node_id, reports) = configure_task(app);
        app.agent_task_coordinator
            .as_mut()
            .expect("应有协调器")
            .dispatch(
                vec![agent::AgentAssignment {
                    target: "Chloe".into(),
                    task: "Interrupted task".into(),
                }],
                false,
            )
            .expect("应派出任务");
        update_task(app, node_id, AgentTaskState::Thinking, "turn/started");
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        update_task(app, node_id, AgentTaskState::Ended, "interrupt");
        app.advance_agent_task_queue(node_id);
        update_task(app, node_id, AgentTaskState::Thinking, "turn/started");
        update_task(
            app,
            node_id,
            AgentTaskState::RunningTool {
                tool_name: Some("shell".into()),
            },
            "item/started",
        );
        if finish_in_batch {
            update_task(app, node_id, AgentTaskState::Done, "turn/completed");
        }
        let events = app.runtime.take_events();
        app.handle_agent_runtime_events(&events, cx);
        let report = reports.try_recv().expect("旧任务必须播报中断");
        assert_eq!(report.text, "Chloe, Interrupted", "旧任务终态应保持中断");
        report.completed.send(()).expect("应确认中断播报完成");
        (node_id, reports)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        if !finish_in_batch {
            update_task(app, node_id, AgentTaskState::Done, "turn/completed");
            let events = app.runtime.take_events();
            app.handle_agent_runtime_events(&events, cx);
        }
        let report = reports
            .try_recv()
            .expect("新续聊的完成名额不能被旧任务抢占");
        assert_eq!(report.text, "Chloe, Done", "新续聊应独立播报完成");
        assert_ne!(
            report.text, "Chloe, Interrupted",
            "旧中断不能重复占用新播报"
        );
        report.completed.send(()).expect("应确认完成播报");
    });
}
