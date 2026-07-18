use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use runtime::{AgentTaskState, RuntimeEvent};
use std::time::Duration;

use crate::harness::{create_codex_agent, echo_factory, enter_canvas, make_app};

fn task_event(node_id: canvas::NodeId, to: AgentTaskState, source: &str) -> RuntimeEvent {
    RuntimeEvent::AgentTaskStateChanged {
        node_id,
        from: AgentTaskState::Ready,
        to,
        source_event: source.to_string(),
    }
}

/// E2E-06 Agent 任务终态经运行时事实生成名称播报并复用实际 PCM 内缘光。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent profile = Codex，呼叫名 alias = Raya
///   round-1 events = UserPromptSubmit + Stop + duplicate Stop
///   fake PCM = 1_600 samples / 16_000Hz = 100ms
///   fade-in checkpoint = 60ms，位于 120ms 淡入中点
///   finish fade = 220ms，等于正常收束完整时长
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Codex Agent 卡片并把呼叫名设为 Raya，启用可控 e2e TTS/playback seam。
///   2. 注入 UserPromptSubmit 与 Stop → 产生 `Raya, Done` 并开始实际 PCM 时间线。
///   3. 注入重复 Stop → 同一轮不得再次播报。
///   4. 完成可控播放并推进 220ms → 内缘光正常收束。
///   5. 清空 alias 后开启第二轮并注入 StopFailure → 回退为 `Codex, Failed`。
///
/// 预期结果：
///   - 正断言：第一轮只出现 `Raya, Done`，第二轮出现 `Codex, Failed`。
///   - 负断言：第一轮不得错误使用 `Codex, Done` 或重复播报。
///   - 正断言：真实 Started 事件使内缘光激活，Finished 后完整收束。
///   - 负断言：播报前后 Dialogue 都保持 idle，不出现 Replying。
#[test]
fn e2e_06_agent_task_terminal_announcements_use_name_dedupe_and_shared_inset() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建 Agent announcement e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);
    app.update(cx, |app, _cx| {
        let node = app
            .runtime
            .document
            .node_mut(node_id)
            .expect("新建的 Codex Agent 节点必须仍存在");
        let CanvasNodeKind::Agent(spec) = &mut node.kind else {
            panic!("新建节点必须是 Agent 类型");
        };
        spec.alias = Some("Raya".to_string());
        app.enable_agent_announcement_test_audio_for_e2e();
    });

    app.update(cx, |app, cx| {
        app.inject_runtime_event_for_e2e(
            task_event(node_id, AgentTaskState::Thinking, "UserPromptSubmit"),
            cx,
        );
        app.inject_runtime_event_for_e2e(task_event(node_id, AgentTaskState::Done, "Stop"), cx);
        app.inject_runtime_event_for_e2e(task_event(node_id, AgentTaskState::Done, "Stop"), cx);
        app.flush_tts_playback_events_for_e2e(cx);
        app.advance_animations_for_e2e(Duration::from_millis(60), cx);
    });
    let spoken = app.read_with(cx, |app, _cx| app.spoken_agent_announcements_for_e2e());
    let voice = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    let playing = app.read_with(cx, |app, _cx| app.voice_window_inset_e2e_snapshot());
    assert_eq!(
        spoken,
        vec!["Raya, Done".to_string()],
        "同一轮 Stop 即使重复到达也必须只播报一次 `Raya, Done`"
    );
    assert!(
        !spoken.iter().any(|utterance| utterance == "Codex, Done"),
        "存在呼叫名 Raya 时不得错误回退成 `Codex, Done`"
    );
    assert!(
        playing.active,
        "Agent PCM 实际开始 60ms 后必须激活共享内缘光"
    );
    assert_eq!(
        voice.dialogue_phase, "idle",
        "Agent 任务回执不得把 Dialogue phase 改为 Replying/Speaking"
    );

    app.update(cx, |app, cx| {
        app.finish_agent_announcement_for_e2e(cx);
        app.flush_tts_playback_events_for_e2e(cx);
        app.advance_animations_for_e2e(Duration::from_millis(220), cx);
        let node = app
            .runtime
            .document
            .node_mut(node_id)
            .expect("第二轮开始前 Agent 节点必须仍存在");
        let CanvasNodeKind::Agent(spec) = &mut node.kind else {
            panic!("第二轮节点必须仍为 Agent 类型");
        };
        spec.alias = None;
        app.inject_runtime_event_for_e2e(
            task_event(node_id, AgentTaskState::Thinking, "UserPromptSubmit"),
            cx,
        );
        app.inject_runtime_event_for_e2e(
            task_event(
                node_id,
                AgentTaskState::Failed {
                    message: "secret provider detail".to_string(),
                },
                "StopFailure",
            ),
            cx,
        );
    });
    let all_spoken = app.read_with(cx, |app, _cx| app.spoken_agent_announcements_for_e2e());
    let settled = app.read_with(cx, |app, _cx| app.voice_window_inset_e2e_snapshot());
    let final_voice = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        all_spoken,
        vec!["Raya, Done".to_string(), "Codex, Failed".to_string()],
        "缺少 alias 的第二轮失败必须使用 runtime profile 并省略失败详情"
    );
    assert!(
        !all_spoken
            .iter()
            .any(|utterance| utterance.contains("secret provider detail")),
        "播报不得泄漏 provider 失败详情"
    );
    assert!(
        !settled.active,
        "第一轮 Finished 后推进完整 220ms 时旧内缘光必须收束；第二轮未 flush 不应提前点亮"
    );
    assert_eq!(
        final_voice.dialogue_phase, "idle",
        "连续两轮 Agent 播报都不得进入 Dialogue Speaking 状态"
    );
}

/// E2E-06-B：单次语音或文字委派遗留的模板回执不得阻塞后续 Agent 终态播报。
///
/// 数据构造（含关键数值的推导过程）：
///   voice interaction = single voice command，Dialogue phase = Idle
///   queued template reports = 1 条派发回执；Idle 阶段无法消费该回执
///   Agent profile = Codex，呼叫名 = Bella
///   task events = 2：UserPromptSubmit + Stop
///   expected announcements = 1：`Bella, Done`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Bella Agent 并启用可控 e2e TTS，保持 Dialogue 为 Idle。
///   2. 模拟自然语言委派完成后遗留 1 条模板回执。
///   3. 注入可信任务开始与 Stop 终态，触发生产 Agent announcement 协调器。
///   4. 读取可控音频出口和 Dialogue 状态。
///
/// 预期结果：
///   - 正断言：即使存在 Idle 阶段无法消费的模板回执，也必须播报一次 `Bella, Done`。
///   - 负断言：不得因模板回执永久等待而得到空播报列表。
///   - 负断言：终态播报不得把 Dialogue 从 Idle 改成 Speaking。
#[test]
fn e2e_06_idle_delegation_report_does_not_block_terminal_announcement() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建单次委派播报 e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);
    app.update(cx, |app, _cx| {
        let node = app
            .runtime
            .document
            .node_mut(node_id)
            .expect("新建的 Bella Agent 节点必须仍存在");
        let CanvasNodeKind::Agent(spec) = &mut node.kind else {
            panic!("新建节点必须是 Agent 类型");
        };
        spec.alias = Some("Bella".to_string());
        app.enable_agent_announcement_test_audio_for_e2e();
        app.queue_dialogue_agent_report_for_e2e("任务已派发给 Bella");
    });

    app.update(cx, |app, cx| {
        app.inject_runtime_event_for_e2e(
            task_event(node_id, AgentTaskState::Thinking, "UserPromptSubmit"),
            cx,
        );
        app.inject_runtime_event_for_e2e(task_event(node_id, AgentTaskState::Done, "Stop"), cx);
    });

    let spoken = app.read_with(cx, |app, _cx| app.spoken_agent_announcements_for_e2e());
    let voice = app.read_with(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        spoken,
        vec!["Bella, Done".to_string()],
        "Idle 阶段无法消费的派发回执不得阻塞可信终态播报，实际：{spoken:?}"
    );
    assert!(
        !spoken.is_empty(),
        "Agent 已可信完成时播报列表不得因遗留模板回执保持为空"
    );
    assert_eq!(
        voice.dialogue_phase, "idle",
        "Agent 终态播报不得把单次委派后的 Dialogue 改成 Speaking"
    );
}
