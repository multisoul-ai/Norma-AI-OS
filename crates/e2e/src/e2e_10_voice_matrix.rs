//! E2E-10 Voice：覆盖当前语音交互模式、配置组合和运行时链路的场景矩阵。

use canvas::CanvasNodeKind;
use gpui::{KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{
    create_terminal, echo_factory, enter_canvas, make_app, read_grid, read_runtime,
    VoiceTranscriptEnvGuard,
};

const TEST_TRANSCRIPT: &str = "voice-matrix-input";

/// Voice 单次语音指令矩阵：Hold 与 Toggle 都必须经真实 ⌘E 快捷键进入共享 Agent 回合。
///
/// 数据构造（含关键数值的推导过程）：
///   interaction modes = 1（SingleVoiceCommand）
///   trigger variants  = 2（Hold + Toggle）
///   terminal targets  = 2（用于验证 Final 不再直写焦点）
///   shortcut downs    = 3（Hold 1 次 + Toggle 启动/停止各 1 次）
///   shortcut ups      = 2（Hold 释放应停止；Toggle 释放不得停止）
///   final routes      = 2（Hold 与 Toggle 均进入 Dialogue Thinking）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在默认 SingleVoiceCommand + Hold 下按下 ⌘E → Voice 进入 recording，Terminal 尚无文本
///   2. 松开快捷键 → 隐藏测试 transcript 作为 Final，Dialogue 进入 Thinking
///   3. 在 Settings 将单次语音指令触发改为 Toggle，新建并聚焦第二个 Terminal
///   4. 第一次按下 ⌘E → recording；松开按键 → 仍保持 recording 且没有提交文本
///   5. 第二次按下 ⌘E → 停止、提交同一 Final，并进入 Dialogue Thinking
///
/// 预期结果：
///   - 正断言：Hold 在 key-up 提交，Toggle 在第二次 key-down 提交
///   - 正断言：两次 Final 都进入共享 Agent 回合
///   - 负断言：Toggle 的第一次 key-up 不得提前提交
///   - 负断言：Final 不得再直写焦点 Terminal
#[test]
fn e2e_10_single_voice_command_hold_and_toggle_shortcuts_enter_shared_agent_turn() {
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
        hold_finished.dialogue_phase, "thinking",
        "Hold Final 应进入共享 Agent 回合，snapshot: {:?}",
        hold_finished
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
        "Hold Final 不得经焦点输入或 PTY 回显写入第一个 Terminal"
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
    let toggle_finished = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        toggle_finished.dialogue_phase, "thinking",
        "Toggle 第二次 key-down 的 Final 应进入共享 Agent 回合，snapshot: {:?}",
        toggle_finished
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
        "Toggle Final 不得写入第二个焦点 Terminal"
    );
    assert_ne!(
        toggle_finished.phase, "recording",
        "Toggle 第二次 key-down 提交后不得继续 recording，snapshot: {:?}",
        toggle_finished
    );

    let terminal_count = read_runtime(&app, cx, |runtime| {
        let terminals = runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .count();
        terminals
    });
    assert_eq!(terminal_count, 2, "两种听写触发应各保留一个目标 Terminal");
}

/// Voice 错误矩阵：单次语音指令不依赖焦点卡片，无模型持续聆听仍必须显示可恢复提示。
///
/// 数据构造（含关键数值的推导过程）：
///   apps = 2（无焦点 SingleVoiceCommand + 无模型 Continuous Listening）
///   focused text targets = 0
///   selected ASR models  = 0
///   implicit nodes       = 0（失败路径不得替用户创建卡片）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在空 Canvas 以 Hold 触发单次语音指令 final → 进入 Agent Thinking，不读取焦点
///   2. 读取底部语音状态与 runtime 文档
///   3. 新建 fresh App，切到 Continuous Listening 后按下 ⌘E
///   4. 后台启动校验发现没有共享识别模型选择 → 返回设置提示，不请求麦克风
///
/// 预期结果：
///   - 正断言：无焦点单次语音指令仍进入 Agent Thinking
///   - 正断言：无模型持续聆听显示 install/select model 提示并保持 dialogue Idle
///   - 负断言：两条失败路径都不得进入 recording/listening 或创建隐式节点
///   - 负断言：模型缺失不是 TCC 错误，不得显示麦克风设置恢复入口
#[test]
fn e2e_10_voice_errors_are_reported_at_single_command_and_model_boundaries() {
    let _transcript = VoiceTranscriptEnvGuard::set(TEST_TRANSCRIPT);

    let mut dictation_cx = TestAppContext::single();
    let (dictation_app, dictation_cx) = make_app(&mut dictation_cx, echo_factory());
    let dictation_workspace = tempfile::TempDir::new().expect("应创建无焦点听写的隔离 workspace");
    enter_canvas(&dictation_app, dictation_cx, dictation_workspace.path());
    voice_key_down(dictation_cx);
    voice_key_up(dictation_cx);

    let no_focus = dictation_app.update(dictation_cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        no_focus.dialogue_phase, "thinking",
        "无焦点单次语音指令应进入 Agent Thinking，snapshot: {:?}",
        no_focus
    );
    assert_ne!(
        no_focus.phase, "recording",
        "单次语音指令提交后不得继续占用 recording"
    );
    assert_eq!(
        read_runtime(&dictation_app, dictation_cx, |runtime| runtime
            .document
            .nodes()
            .len()),
        0,
        "无焦点单次语音指令不得隐式创建 Terminal 或 Agent"
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

/// Voice 回合错误关闭：错误态必须提供独立关闭操作，且关闭只清错误、不误启动录音。
///
/// 数据构造（含关键数值的推导过程）：
///   failed voice turns = 1（模拟 `MaxTurnsError` 的单次语音 Agent 回合）
///   dismiss clicks     = 1（用户只点击明确的关闭按钮）
///   recording starts   = 0（关闭错误不是开始录音）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless GPUI App 并进入 Canvas。
///   2. 通过 e2e seam 构造 SingleVoiceCommand 的 Agent 回合失败。
///   3. 从真实渲染树定位专用关闭按钮并点击。
///   4. 读取语音快照，验证错误视觉和错误正文均已清除。
///
/// 预期结果：
///   - 正断言：错误态渲染独立、可点击的 `voice-error-dismiss`。
///   - 正断言：点击后语音入口恢复 Idle 视觉，底层 one-shot 状态仍为 Executed。
///   - 负断言：关闭错误不得误开始 Recording。
///   - 负断言：关闭后不得继续显示旧 `MaxTurnsError`。
#[test]
fn e2e_10_voice_turn_error_is_independently_dismissible() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建语音错误关闭的隔离 workspace");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.show_single_voice_turn_error_for_e2e(
            "Turn error: Norma provider or tool loop failed: MaxTurnsError",
        );
        cx.notify();
    });
    cx.run_until_parked();

    let before = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        before.visual, "error",
        "注入 Agent 回合失败后语音入口必须显示 error 视觉，snapshot: {before:?}"
    );
    let dismiss = cx
        .debug_bounds("voice-error-dismiss")
        .expect("语音错误态必须渲染独立的 voice-error-dismiss 点击区域");
    cx.simulate_click(dismiss.center(), Modifiers::none());
    cx.run_until_parked();

    let dismissed = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        dismissed.visual, "idle",
        "关闭语音错误后必须恢复 Idle 视觉，snapshot: {dismissed:?}"
    );
    assert_eq!(
        dismissed.phase, "executed",
        "关闭错误只清反馈，不得把上一轮完成态伪装成新录音，snapshot: {dismissed:?}"
    );
    assert_ne!(
        dismissed.phase, "recording",
        "点击关闭按钮不得冒充语音入口并误启动录音"
    );
    assert!(
        !dismissed.label.contains("MaxTurnsError"),
        "关闭后不得继续显示旧 MaxTurnsError，snapshot: {dismissed:?}"
    );
}

/// Voice 回合错误恢复：不先关闭时再次按 ⌘E 也必须直接开始可见的新录音。
///
/// 数据构造（含关键数值的推导过程）：
///   failed voice turns = 1（上一轮留下 `MaxTurnsError`）
///   shortcut downs     = 1（下一轮唯一一次 ⌘E key-down）
///   transcript source  = `voice-matrix-input`（阻止测试访问真实麦克风）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 App、进入 Canvas，并注入上一轮 Agent 错误。
///   2. 用户不点击关闭，直接按下一次 ⌘E。
///   3. 读取 VoiceControl 与最终渲染视觉/正文。
///
/// 预期结果：
///   - 正断言：一次 ⌘E 后 VoiceControl 进入 Recording。
///   - 正断言：最终视觉同步进入 Recording，并显示 Listening。
///   - 负断言：新录音不得继续被上一轮错误视觉和 `MaxTurnsError` 文案遮住。
#[test]
fn e2e_10_voice_shortcut_recovers_immediately_after_turn_error() {
    let _transcript = VoiceTranscriptEnvGuard::set(TEST_TRANSCRIPT);
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建语音快捷键恢复的隔离 workspace");
    enter_canvas(&app, cx, workspace.path());

    app.update(cx, |app, cx| {
        app.show_single_voice_turn_error_for_e2e(
            "Turn error: Norma provider or tool loop failed: MaxTurnsError",
        );
        cx.notify();
    });
    cx.run_until_parked();

    voice_key_down(cx);

    let recording = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        recording.phase, "recording",
        "错误后的第一次 ⌘E 必须开始新录音，snapshot: {recording:?}"
    );
    assert_eq!(
        recording.visual, "recording",
        "新录音必须立即显示 Recording 视觉，不能继续被旧错误覆盖，snapshot: {recording:?}"
    );
    assert!(
        recording.label.contains("Listening"),
        "新录音应显示 Listening 反馈，snapshot: {recording:?}"
    );
    assert!(
        !recording.label.contains("MaxTurnsError"),
        "新录音不得继续显示上一轮 MaxTurnsError，snapshot: {recording:?}"
    );
}

/// Continuous Listening 错误恢复：会话仍在收音时，⌘E 应清错而不是误停止会话。
///
/// 数据构造（含关键数值的推导过程）：
///   interaction mode = ContinuousListening
///   dialogue phase   = Listening（回合失败后麦克风会话仍存活）
///   visible errors   = 1（`MaxTurnsError` 覆盖 Recording 视觉）
///   shortcut downs   = 1（用户尝试恢复录音反馈）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Settings 把交互模式切为 Continuous Listening。
///   2. 注入回合失败，使底层保持 Listening、顶部显示 Error。
///   3. 用户直接按一次 ⌘E，不先点击关闭按钮。
///   4. 读取 Dialogue phase 与最终语音视觉。
///
/// 预期结果：
///   - 正断言：一次 ⌘E 后 Dialogue 仍为 Listening，Recording 视觉重新可见。
///   - 正断言：状态文案恢复 Continuous Listening。
///   - 负断言：快捷键不得因错误覆盖而把存活会话误停到 Idle。
///   - 负断言：恢复后不得继续显示旧 `MaxTurnsError`。
#[test]
fn e2e_10_continuous_voice_shortcut_reveals_surviving_listening_after_turn_error() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建持续聆听错误恢复的隔离 workspace");
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click_debug(cx, "settings-section-voice");
    click_debug(cx, "settings-voice-mode-continuous");
    cx.simulate_keystrokes("cmd-,");

    app.update(cx, |app, cx| {
        app.show_continuous_voice_turn_error_for_e2e(
            "Turn error: Norma provider or tool loop failed: MaxTurnsError",
        );
        cx.notify();
    });
    cx.run_until_parked();
    let failed = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        failed.dialogue_phase, "listening",
        "回合失败后底层 Continuous Listening 应继续存活，snapshot: {failed:?}"
    );
    assert_eq!(
        failed.visual, "error",
        "旧错误应先覆盖 Listening 视觉，证明测试命中截图中的状态，snapshot: {failed:?}"
    );

    voice_key_down(cx);

    let recovered = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    assert_eq!(
        recovered.dialogue_phase, "listening",
        "错误态下一次 ⌘E 应恢复可见录音，不得停止存活的 Continuous Listening，snapshot: {recovered:?}"
    );
    assert_eq!(
        recovered.visual, "recording",
        "错误态下一次 ⌘E 应重新露出 Recording 视觉，snapshot: {recovered:?}"
    );
    assert_eq!(
        recovered.label, "Continuous Listening",
        "清除错误后应显示存活会话的 Continuous Listening 文案，snapshot: {recovered:?}"
    );
    assert_ne!(
        recovered.dialogue_phase, "idle",
        "错误恢复快捷键不得把仍在收音的会话误停到 Idle"
    );
    assert!(
        !recovered.label.contains("MaxTurnsError"),
        "恢复后不得继续显示旧 MaxTurnsError，snapshot: {recovered:?}"
    );
}

fn voice_keystroke() -> Keystroke {
    Keystroke {
        modifiers: Modifiers {
            platform: true,
            ..Modifiers::default()
        },
        key: "e".to_string(),
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
