use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use runtime::{AgentTaskState, RuntimeEvent};

use crate::harness::{create_codex_agent, echo_factory, enter_canvas, make_app};

fn task_event(node_id: canvas::NodeId, to: AgentTaskState, source: &str) -> RuntimeEvent {
    RuntimeEvent::AgentTaskStateChanged {
        node_id,
        from: AgentTaskState::Ready,
        to,
        source_event: source.to_string(),
    }
}

/// E2E-06-F：未分类 VAD 起点不能取消正在播放的 Agent 终态通知。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent profile = Codex，呼叫名 = Henry
///   task events = 2：UserPromptSubmit + Stop/Done
///   microphone events = 1 条 unclassified SpeechStart
///   active announcements before/after = true → true
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Henry Agent，启用可控 TTS 并注入可信完成事件 → `Henry, Done` 开始播放。
///   2. 注入尚未被识别为用户语音的 VAD SpeechStart → Dialogue 执行未分类声音处理。
///   3. 读取播报活跃状态、通知结果和已播文本。
///
/// 预期结果：
///   - 正断言：SpeechStart 前后 Agent 播报都保持活跃。
///   - 负断言：未分类声音不得被当成确认插话，也不得清除 `Henry, Done`。
///   - 正断言：播报文本仍恰好只有一条 `Henry, Done`。
#[test]
fn e2e_06_unclassified_speech_start_preserves_active_agent_announcement() {
    let mut test_cx = TestAppContext::single();
    let (app, cx) = make_app(&mut test_cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建 VAD interruption e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);
    app.update(cx, |app, _cx| {
        let node = app
            .runtime
            .document
            .node_mut(node_id)
            .expect("新建的 Henry Agent 节点必须仍存在");
        let CanvasNodeKind::Agent(spec) = &mut node.kind else {
            panic!("新建节点必须是 Agent 类型");
        };
        spec.alias = Some("Henry".to_string());
        app.enable_agent_announcement_test_audio_for_e2e();
    });
    app.update(cx, |app, cx| {
        app.inject_runtime_event_for_e2e(
            task_event(node_id, AgentTaskState::Thinking, "UserPromptSubmit"),
            cx,
        );
        app.inject_runtime_event_for_e2e(task_event(node_id, AgentTaskState::Done, "Stop"), cx);
    });

    let active_before = app.read_with(cx, |app, _cx| app.agent_announcement_active_for_e2e());
    let should_notify = app.update(cx, |app, _cx| app.unclassified_speech_start_for_e2e());
    let active_after = app.read_with(cx, |app, _cx| app.agent_announcement_active_for_e2e());
    let spoken = app.read_with(cx, |app, _cx| app.spoken_agent_announcements_for_e2e());

    assert!(
        active_before,
        "注入未分类 SpeechStart 前 `Henry, Done` 必须已经处于活跃播放状态"
    );
    assert!(
        active_after,
        "未分类 SpeechStart 不得取消正在播放的 Agent 终态通知"
    );
    assert!(!should_notify, "未分类声音不能被误判为已确认的用户插话");
    assert_eq!(
        spoken,
        vec!["Henry, Done".to_string()],
        "未分类 SpeechStart 后播报记录必须仍恰好只有 `Henry, Done`"
    );
}
