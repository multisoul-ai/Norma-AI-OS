//! E2E-10 Voice：覆盖当前语音交互模式、配置组合和运行时链路的场景矩阵。

use canvas::CanvasNodeKind;
use gpui::{KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{
    create_terminal, drain_until, echo_factory, enter_canvas, make_app, read_grid, read_runtime,
};

const TEST_TRANSCRIPT: &str = "voice-matrix-input";
static VOICE_TRANSCRIPT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Voice 听写矩阵：Hold 与 Toggle 都必须经真实快捷键写入焦点卡片，且绕过 Agent。
///
/// 数据构造（含关键数值的推导过程）：
///   interaction modes = 1（Dictation）
///   trigger variants  = 2（Hold + Toggle）
///   terminal targets  = 2（每种触发各一个独立焦点 Terminal）
///   shortcut downs    = 3（Hold 1 次 + Toggle 启动/停止各 1 次）
///   shortcut ups      = 2（Hold 释放应停止；Toggle 释放不得停止）
///   agent nodes       = 0（听写不得创建或调用 Agent）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在默认 Dictation + Hold 下按下 ⌘⇧V → Voice 进入 recording，Terminal 尚无文本
///   2. 松开快捷键 → 隐藏测试 transcript 作为 final，经焦点输入路由写入第一个 Terminal
///   3. 在 Settings 将听写触发改为 Toggle，新建并聚焦第二个 Terminal
///   4. 第一次按下 ⌘⇧V → recording；松开按键 → 仍保持 recording 且没有提交文本
///   5. 第二次按下 ⌘⇧V → 停止并将同一 final transcript 写入第二个 Terminal
///
/// 预期结果：
///   - 正断言：Hold 在 key-up 提交，Toggle 在第二次 key-down 提交
///   - 正断言：两次听写都进入真实 Terminal 输入/PTY 回显闭环
///   - 负断言：Toggle 的第一次 key-up 不得提前提交
///   - 负断言：听写全程不得创建 Agent 节点或进入持续聆听状态
#[test]
fn e2e_10_dictation_hold_and_toggle_shortcuts_insert_into_focused_terminal_only() {
    let _transcript = VoiceTranscriptEnvGuard::set(TEST_TRANSCRIPT);
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建隔离的语音 e2e workspace");
    enter_canvas(&app, cx, workspace.path());

    let hold_terminal = create_terminal(&app, cx);
    voice_key_down(cx);
    let hold_recording = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        hold_recording.phase, "recording",
        "Hold 按下后必须进入 recording，snapshot: {:?}",
        hold_recording
    );
    assert!(
        !read_grid(&app, cx, hold_terminal)
            .expect("Hold 目标应保持可读的 Terminal grid")
            .rows
            .iter()
            .flatten()
            .map(|cell| cell.c)
            .collect::<String>()
            .contains(TEST_TRANSCRIPT),
        "Hold 尚未松开时不得提前把 final transcript 写入 Terminal"
    );

    voice_key_up(cx);
    let hold_finished = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        hold_finished.label, "Dictation inserted",
        "Hold final 应成功路由到焦点文本目标，snapshot: {:?}",
        hold_finished
    );
    assert!(
        drain_until(
            &app,
            cx,
            |runtime| {
                let grid = match runtime.node_view(hold_terminal) {
                    Some(runtime::NodeView::Term { snapshot, .. }) => Some(snapshot),
                    _ => None,
                };
                grid.filter(|grid| grid_contains(grid, TEST_TRANSCRIPT))
            },
            std::time::Duration::from_secs(1),
        )
        .is_some(),
        "Hold 松开后 transcript 应经焦点输入和 PTY 回显出现在第一个 Terminal"
    );
    assert_ne!(
        hold_finished.phase, "recording",
        "Hold 松开并提交后不得继续占用 recording，snapshot: {:?}",
        hold_finished
    );

    cx.simulate_keystrokes("cmd-,");
    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-trigger-toggle");
    let toggle_settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        toggle_settings.dictation_trigger, "toggle",
        "用户点击 Toggle 后设置必须立即生效，snapshot: {:?}",
        toggle_settings
    );
    assert_ne!(
        toggle_settings.dictation_trigger, "hold",
        "Toggle 配置不得继续报告 Hold"
    );
    cx.simulate_keystrokes("cmd-,");

    let toggle_terminal = create_terminal(&app, cx);
    voice_key_down(cx);
    voice_key_up(cx);
    let toggle_after_release = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        toggle_after_release.phase, "recording",
        "Toggle 第一次 key-up 后仍应录音，snapshot: {:?}",
        toggle_after_release
    );
    assert!(
        !read_grid(&app, cx, toggle_terminal)
            .expect("Toggle 目标应保持可读的 Terminal grid")
            .rows
            .iter()
            .flatten()
            .map(|cell| cell.c)
            .collect::<String>()
            .contains(TEST_TRANSCRIPT),
        "Toggle 第一次 key-up 不得提前提交 transcript"
    );

    voice_key_down(cx);
    assert!(
        drain_until(
            &app,
            cx,
            |runtime| {
                let grid = match runtime.node_view(toggle_terminal) {
                    Some(runtime::NodeView::Term { snapshot, .. }) => Some(snapshot),
                    _ => None,
                };
                grid.filter(|grid| grid_contains(grid, TEST_TRANSCRIPT))
            },
            std::time::Duration::from_secs(1),
        )
        .is_some(),
        "Toggle 第二次 key-down 后 transcript 应出现在第二个 Terminal"
    );
    let toggle_finished = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_ne!(
        toggle_finished.phase, "recording",
        "Toggle 第二次 key-down 提交后不得继续 recording，snapshot: {:?}",
        toggle_finished
    );

    let (terminal_count, agent_count) = read_runtime(&app, cx, |runtime| {
        let terminals = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .count();
        let agents = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count();
        (terminals, agents)
    });
    assert_eq!(terminal_count, 2, "两种听写触发应各保留一个目标 Terminal");
    assert_eq!(
        agent_count, 0,
        "Dictation 不得创建 Agent 节点或转入 Agent 路径"
    );
}

/// Voice 错误矩阵：无焦点听写与无模型持续聆听必须失败在正确边界并给出可恢复提示。
///
/// 数据构造（含关键数值的推导过程）：
///   apps = 2（无焦点 Dictation + 无模型 Continuous Listening）
///   focused text targets = 0
///   selected ASR models  = 0
///   implicit nodes       = 0（失败路径不得替用户创建卡片）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在空 Canvas 以 Hold 触发听写 final → 焦点输入路由找不到文本目标
///   2. 读取底部语音状态与 runtime 文档
///   3. 新建 fresh App，切到 Continuous Listening 后按下 ⌘⇧V
///   4. 后台启动校验发现没有共享识别模型选择 → 返回设置提示，不请求麦克风
///
/// 预期结果：
///   - 正断言：无焦点听写显示 Focus a text-capable card 错误
///   - 正断言：无模型持续聆听显示 install/select model 提示并保持 dialogue Idle
///   - 负断言：两条失败路径都不得进入 recording/listening 或创建隐式节点
///   - 负断言：模型缺失不是 TCC 错误，不得显示麦克风设置恢复入口
#[test]
fn e2e_10_voice_errors_are_reported_at_focus_and_model_boundaries() {
    let _transcript = VoiceTranscriptEnvGuard::set(TEST_TRANSCRIPT);

    let mut dictation_cx = TestAppContext::single();
    let (dictation_app, dictation_cx) = make_app(&mut dictation_cx, echo_factory());
    let dictation_workspace = tempfile::TempDir::new().expect("应创建无焦点听写的隔离 workspace");
    enter_canvas(&dictation_app, dictation_cx, dictation_workspace.path());
    voice_key_down(dictation_cx);
    voice_key_up(dictation_cx);

    let no_focus = dictation_app.update(dictation_cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(no_focus.phase, "error", "无焦点听写应进入可见 Error 状态");
    assert!(
        no_focus.label.contains("Focus a text-capable card"),
        "无焦点听写应告诉用户先聚焦文本卡片，snapshot: {:?}",
        no_focus
    );
    assert_ne!(
        no_focus.phase, "recording",
        "听写失败后不得继续占用 recording"
    );
    assert_eq!(
        read_runtime(&dictation_app, dictation_cx, |runtime| runtime
            .document
            .nodes()
            .len()),
        0,
        "无焦点听写不得隐式创建 Terminal 或 Agent"
    );

    let mut continuous_cx = TestAppContext::single();
    let (continuous_app, continuous_cx) = make_app(&mut continuous_cx, echo_factory());
    let continuous_workspace =
        tempfile::TempDir::new().expect("应创建无模型持续聆听的隔离 workspace");
    enter_canvas(&continuous_app, continuous_cx, continuous_workspace.path());
    continuous_cx.simulate_keystrokes("cmd-,");
    click_debug(continuous_cx, "settings-section-voice");
    click_debug(continuous_cx, "settings-voice-mode-continuous");
    continuous_cx.simulate_keystrokes("cmd-,");
    voice_key_down(continuous_cx);

    let no_model = continuous_app.update(continuous_cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        no_model.dialogue_phase, "idle",
        "没有模型时 Continuous Listening 必须保持 Idle，snapshot: {:?}",
        no_model
    );
    assert!(
        no_model.label.contains("no ASR model selected"),
        "无模型错误应引导 install/select model，snapshot: {:?}",
        no_model
    );
    assert_ne!(
        no_model.dialogue_phase, "listening",
        "模型校验失败后不得伪装成 Continuous Listening 已启动"
    );
    assert!(
        !no_model.microphone_recovery_available,
        "模型缺失不是麦克风权限错误，不得显示 Open Microphone Settings"
    );
}

fn voice_keystroke() -> Keystroke {
    Keystroke {
        modifiers: Modifiers {
            platform: true,
            shift: true,
            ..Modifiers::default()
        },
        key: "v".to_string(),
        key_char: None,
    }
}

fn voice_key_down(cx: &mut VisualTestContext) {
    cx.simulate_event(KeyDownEvent {
        keystroke: voice_keystroke(),
        is_held: false,
    });
    cx.run_until_parked();
}

fn voice_key_up(cx: &mut VisualTestContext) {
    cx.simulate_event(KeyUpEvent {
        keystroke: voice_keystroke(),
    });
    cx.run_until_parked();
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应有可点击 bounds"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn grid_contains(grid: &terminal::GridSnapshot, expected: &str) -> bool {
    grid.rows
        .iter()
        .flatten()
        .map(|cell| cell.c)
        .collect::<String>()
        .contains(expected)
}

struct VoiceTranscriptEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
}

impl VoiceTranscriptEnvGuard {
    fn set(transcript: &str) -> Self {
        let lock = VOICE_TRANSCRIPT_ENV_LOCK
            .lock()
            .expect("语音 transcript 环境变量测试锁不应中毒");
        let previous = std::env::var_os("MULTISOUL_VOICE_TRANSCRIPT");
        std::env::set_var("MULTISOUL_VOICE_TRANSCRIPT", transcript);
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for VoiceTranscriptEnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::env::set_var("MULTISOUL_VOICE_TRANSCRIPT", previous);
        } else {
            std::env::remove_var("MULTISOUL_VOICE_TRANSCRIPT");
        }
    }
}
