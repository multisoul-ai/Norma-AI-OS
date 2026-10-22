use super::*;
use crate::settings::voice::{
    DesktopVoiceMode, EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState,
};
use canvas::{AgentProfile, AgentSpec, CanvasRect};
use gpui::TestAppContext;
use runtime::{AgentTaskState, RuntimeEvent};
use tokio_util::sync::CancellationToken;

fn task_event(node_id: u64, to: AgentTaskState, source_event: &str) -> RuntimeEvent {
    RuntimeEvent::AgentTaskStateChanged {
        node_id,
        from: AgentTaskState::Ready,
        to,
        source_event: source_event.into(),
    }
}

/// 豆包续聊回合：没有派工 task_id 时，真实卡片 Stop 仍产生唯一完成播报。
///
/// 数据构造（含关键数值的推导过程）：
///   AgentCard = 1（Chloe/Claude）；协调器任务 = 0，模拟 reply_agent 只有 reply_id 的路径。
///   事件 = 1 次 UserPromptSubmit + 2 次 Stop；每轮通知预算 = 1 条。
///   卡片几何 = 640 × 360，仅提供合法画布区域，不影响完成判断。
///
/// 执行过程：
///   1. 创建隔离 App，选择豆包并接入本地汇报通道，不连接真实网络或麦克风。
///   2. 打开卡片回合 → 不能播报完成。
///   3. 输入真实 Stop/Done → 检查豆包通道收到 Chloe 的完成消息。
///   4. 重复 Stop → 不得生成第二条播报。
///
/// 预期结果：Chloe 完成消息确实进入豆包输出边界；开始与重复终态均不能发声。
#[test]
fn doubao_reply_turn_announces_completion_without_coordinator_task() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建隔离工作区");
    let (host, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let (sender, reports) = async_channel::unbounded();
    let node_id = host.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("synthetic-doubao-account".into()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.doubao.reports = Some(sender);
        app.doubao.cancellation = Some(CancellationToken::new());
        let node_id = app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
                alias: Some("Chloe".into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
        assert!(
            app.agent_task_coordinator.is_none(),
            "续聊场景不能依赖派工协调器"
        );
        app.handle_agent_runtime_events(
            &[task_event(
                node_id,
                AgentTaskState::Thinking,
                "UserPromptSubmit",
            )],
            cx,
        );
        assert!(reports.try_recv().is_err(), "回合刚开始时不能播报完成");
        app.handle_agent_runtime_events(&[task_event(node_id, AgentTaskState::Done, "Stop")], cx);
        let report = reports
            .try_recv()
            .expect("续聊的真实 Stop 必须进入豆包播报通道");
        assert_eq!(report.text, "Chloe, Done", "播报必须属于完成的 Chloe 卡片");
        report.completed.send(()).expect("应确认实际播完");
        node_id
    });
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.handle_agent_runtime_events(&[task_event(node_id, AgentTaskState::Done, "Stop")], cx);
        assert!(
            reports.try_recv().is_err(),
            "重复 Stop 不得生成第二条完成播报"
        );
    });
}

/// 豆包断线：已发送但尚未播完的完成通知必须保留，重连后补播一次。
///
/// 数据构造（含关键数值的推导过程）：
///   AgentCard = 1（Chloe）；完成事件 = 1；旧汇报通道 + 新汇报通道 = 2。
///   旧通道收到通知后不消费，直接断开，模拟通知入队与 TTS 播完之间的故障。
///   卡片几何 = 640 × 360，只保证合法画布区域；没有真实时间或网络依赖。
///
/// 执行过程：
///   1. 选择豆包，把 Chloe 的 Stop/Done 派往旧通道。
///   2. 停止通话并丢弃旧接收端 → 原通知没有播完。
///   3. 推进后台回执处理，接入新通道并再次运行通知调度。
///
/// 预期结果：新通道收到原 Chloe 完成通知；未播放不能被当成已通知并永久丢失。
#[test]
fn doubao_unplayed_completion_survives_disconnect() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建隔离工作区");
    let (host, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let (sender, old_reports) = async_channel::unbounded();
    host.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("synthetic-doubao-account".into()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.doubao.reports = Some(sender);
        app.doubao.cancellation = Some(CancellationToken::new());
        let node_id = app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Chloe".into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
        app.handle_agent_runtime_events(&[task_event(node_id, AgentTaskState::Done, "Stop")], cx);
        assert_eq!(old_reports.len(), 1, "旧通道应确实收到一条待播通知");
        app.stop_doubao_voice(cx);
    });
    drop(old_reports);
    cx.run_until_parked();
    let (sender, reports) = async_channel::unbounded();
    host.update(cx, |app, cx| {
        app.doubao.reports = Some(sender);
        app.doubao.cancellation = Some(CancellationToken::new());
        app.handle_agent_runtime_events(&[], cx);
        let report = reports
            .try_recv()
            .expect("重连必须补播没有播放完成的原通知");
        assert_eq!(report.text, "Chloe, Done", "重连应保留原卡片完成通知");
        report.completed.send(()).expect("应确认补播完成");
        assert!(reports.try_recv().is_err(), "重连不能重复派发同一条通知");
    });
}

/// Codex app-server 连续回合：真实 turn/* 事件产生各轮唯一的成功/失败播报。
///
/// 数据构造（含关键数值的推导过程）：
///   AgentCard = 1（Chloe/Codex）；派工协调器任务 = 0。
///   两轮 = turn/started + turn/completed(Done)，再 turn/started + turn/completed(Failed)。
///   每轮重复一次终态，总期望 = 2 轮 × 每轮 1 条 = 2 条；矩形 640 × 360 是合法卡片区域。
///
/// 执行过程：
///   1. 创建隔离 App，连接本地豆包汇报通道，打开 Codex 第一轮。
///   2. 第一轮完成 → 收到 Chloe, Done；确认播完后重复终态不得再播。
///   3. 第二轮开始 → 重置去重状态；失败 → 收到 Chloe, Failed。
///   4. 确认第二轮播完，再输入重复失败 → 不得再播。
///
/// 预期结果：两轮通知均确实发往豆包；开始事件与重复终态均无额外通知。
#[test]
fn doubao_codex_turns_announce_success_and_failure_once() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建隔离工作区");
    let (host, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let (sender, reports) = async_channel::unbounded();
    let node_id = host.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("synthetic-doubao-account".into()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.doubao.reports = Some(sender);
        app.doubao.cancellation = Some(CancellationToken::new());
        let node_id = app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Codex,
                alias: Some("Chloe".into()),
                definition_id: None,
                mobile_control: true,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
        app.handle_agent_runtime_events(
            &[task_event(
                node_id,
                AgentTaskState::Thinking,
                "turn/started",
            )],
            cx,
        );
        assert!(reports.try_recv().is_err(), "Codex 开始不应播报终态");
        app.handle_agent_runtime_events(
            &[task_event(node_id, AgentTaskState::Done, "turn/completed")],
            cx,
        );
        let report = reports
            .try_recv()
            .expect("Codex app-server 完成必须进入豆包通知通道");
        assert_eq!(report.text, "Chloe, Done", "第一轮应播报真实完成结果");
        report.completed.send(()).expect("应确认第一轮播完");
        node_id
    });
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.handle_agent_runtime_events(
            &[task_event(node_id, AgentTaskState::Done, "turn/completed")],
            cx,
        );
        assert!(reports.try_recv().is_err(), "重复完成不能再播第一轮");
        app.handle_agent_runtime_events(
            &[task_event(
                node_id,
                AgentTaskState::Thinking,
                "turn/started",
            )],
            cx,
        );
        assert!(reports.try_recv().is_err(), "第二轮开始不能播报终态");
        app.handle_agent_runtime_events(
            &[task_event(
                node_id,
                AgentTaskState::Failed {
                    message: "service failed".into(),
                },
                "turn/completed",
            )],
            cx,
        );
        let report = reports.try_recv().expect("第二轮 Codex 失败也必须通知");
        assert_eq!(report.text, "Chloe, Failed", "第二轮不能复用上一轮成功结果");
        report.completed.send(()).expect("应确认第二轮播完");
    });
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.handle_agent_runtime_events(
            &[task_event(
                node_id,
                AgentTaskState::Failed {
                    message: "duplicate".into(),
                },
                "turn/completed",
            )],
            cx,
        );
        assert!(reports.try_recv().is_err(), "重复失败不能再播第二轮");
    });
}

/// 豆包离线等待中的通知：关闭播报后重连不能播放旧通知，重新开启只播新回合。
///
/// 数据构造（含关键数值的推导过程）：
///   AgentCard = 1（Chloe/Claude），离线旧回合与重连新回合共 2 轮。
///   旧通知在关闭开关前已完成但未连接；期望播放数 = 旧回合 0 + 新回合 1 = 1。
///   矩形 640 × 360 仅提供合法卡片区域；本地通道无真实网络或麦克风。
///
/// 执行过程：
///   1. 有豆包配置但未连接时完成任务 → 留待重连，不能开始播放。
///   2. 直接同步关闭播报设置，再接入通道 → 丢弃旧通知。
///   3. 重新开启播报 → 仍不恢复旧通知；新回合完成 → 正常播放 Chloe, Done。
///
/// 预期结果：关闭开关确实禁止并清理待播通知；重新开启后新通知仍可正常发送。
#[test]
fn doubao_deferred_notification_is_discarded_when_disabled() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建隔离工作区");
    let (host, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let (sender, reports) = async_channel::unbounded();
    host.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("synthetic-doubao-account".into()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        let node_id = app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
                alias: Some("Chloe".into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
        app.handle_agent_runtime_events(&[task_event(node_id, AgentTaskState::Done, "Stop")], cx);
        assert!(
            app.agent_announcement_audio_idle(),
            "未连接豆包时只能等待，不能开始播放"
        );
        app.voice_settings.toggle_agent_task_announcements();
        app.doubao.reports = Some(sender);
        app.doubao.cancellation = Some(CancellationToken::new());
        app.handle_agent_runtime_events(&[], cx);
        assert!(
            reports.try_recv().is_err(),
            "关闭播报后重连不得发送离线旧通知"
        );
        app.voice_settings.toggle_agent_task_announcements();
        app.handle_agent_runtime_events(&[], cx);
        assert!(
            reports.try_recv().is_err(),
            "重新开启播报不能恢复已丢弃的旧通知"
        );
        app.handle_agent_runtime_events(
            &[
                task_event(node_id, AgentTaskState::Thinking, "UserPromptSubmit"),
                task_event(node_id, AgentTaskState::Done, "Stop"),
            ],
            cx,
        );
        let report = reports.try_recv().expect("重新开启后新回合完成应正常通知");
        assert_eq!(report.text, "Chloe, Done", "新回合应播报真实卡片结果");
        report.completed.send(()).expect("应确认新回合播完");
        assert!(reports.try_recv().is_err(), "新回合不能产生重复通知");
    });
}

/// 豆包通道因鉴权失败被阻断时，完成通知不再排队，避免修好后一次性补播。
///
/// 数据构造：豆包模式 + 已保存凭据；通道未连接且被标记为阻断（鉴权失败）。
///   AgentCard = 1（Chloe/Claude）；矩形 640 × 360 仅提供合法卡片区域。
/// 执行过程：完成任务 → 之后接入通道并再次调度。
/// 预期结果：阻断期间的完成通知被跳过；接入通道后不会补播。
#[test]
fn doubao_blocked_channel_skips_announcements() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建隔离工作区");
    let (host, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let (sender, reports) = async_channel::unbounded();
    host.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("synthetic-doubao-account".into()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.doubao.block_for_test();
        let node_id = app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
                alias: Some("Chloe".into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
        app.handle_agent_runtime_events(&[task_event(node_id, AgentTaskState::Done, "Stop")], cx);
        app.doubao.reports = Some(sender);
        app.doubao.cancellation = Some(CancellationToken::new());
        app.handle_agent_runtime_events(&[], cx);
        assert!(
            reports.try_recv().is_err(),
            "阻断期间的完成通知不能在恢复后补播"
        );
    });
}

/// 后台项目的通道断开后无法重连，完成通知不再排队，避免过期后丢失或回到前台时补播。
///
/// 数据构造：豆包模式 + 已保存凭据；项目已转入后台且通道未连接。
///   AgentCard = 1（Chloe/Claude）；矩形 640 × 360 仅提供合法卡片区域。
/// 执行过程：完成任务 → 之后接入通道并再次调度。
/// 预期结果：后台无通道期间的完成通知被跳过；接入通道后不会补播。
#[test]
fn doubao_background_project_without_channel_skips_announcements() {
    let mut gpui = TestAppContext::single();
    let workspace = tempfile::tempdir().expect("应创建隔离工作区");
    let (host, cx) = gpui.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
        )
    });
    host.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });
    let (sender, reports) = async_channel::unbounded();
    host.update(cx, |app, cx| {
        app.voice_settings = VoiceSettingsState::from_sources(
            SavedVoiceSettings {
                desktop_mode: Some(DesktopVoiceMode::Doubao),
                doubao_credential_ref: Some("synthetic-doubao-account".into()),
                ..Default::default()
            },
            EnvVoiceSettings::default(),
        );
        app.background_project = true;
        let node_id = app.runtime.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile: AgentProfile::Claude,
                alias: Some("Chloe".into()),
                definition_id: None,
                mobile_control: false,
            }),
            CanvasRect::new(0.0, 0.0, 640.0, 360.0),
        );
        app.handle_agent_runtime_events(&[task_event(node_id, AgentTaskState::Done, "Stop")], cx);
        app.doubao.reports = Some(sender);
        app.doubao.cancellation = Some(CancellationToken::new());
        app.handle_agent_runtime_events(&[], cx);
        assert!(
            reports.try_recv().is_err(),
            "后台无通道期间的完成通知不能在恢复后补播"
        );
    });
}
