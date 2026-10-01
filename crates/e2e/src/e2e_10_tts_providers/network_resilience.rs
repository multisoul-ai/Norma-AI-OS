use crate::harness::{echo_factory, enter_canvas, make_app};
use gpui::{Modifiers, TestAppContext};

/// E2E-TTS-NETWORK-1：MiniMax 临时网络故障不得把已配置 Voice 送回 Setup。
///
/// 数据构造（含关键数值的推导过程）：
///   persisted credential = 1（credential ref + SQLite secret）
///   configured Voice gates = 4 / 4
///   injected failures = 1（TemporarilyUnavailable）
///   Settings navigation clicks = 1（Voice section）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App，注入已完成的 MiniMax 配置与一次临时网络故障。
///   2. 进入 Canvas，打开 Settings 并真实点击 Voice 导航。
///   3. 读取 Voice 入口与 TTS Provider 的公开 e2e 快照。
///   4. 检查真实渲染树保留 Ready 概览，且没有重新出现 Setup 标题和 Finish Setup。
///
/// 预期结果：
///   - 正断言：Voice 入口仍为 ready，MiniMax 细分状态为 temporarily_unavailable。
///   - 正断言：凭据仍标记为 configured，Ready banner 继续存在。
///   - 负断言：临时网络故障不得显示 Voice Setup 或 Finish Setup。
#[test]
fn e2e_minimax_network_failure_preserves_configuration_without_reopening_setup() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建 MiniMax 网络故障 e2e workspace");
    app.update(cx, |app, cx| {
        app.assume_minimax_temporary_network_failure_for_e2e(cx);
    });
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");

    let voice = app.update(cx, |app, _cx| app.voice_e2e_snapshot());
    let settings = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));

    assert_eq!(
        voice.entry_state, "ready",
        "MiniMax 临时网络故障不得撤销已完成的 Voice 配置状态"
    );
    assert_eq!(
        settings.tts_provider.minimax_readiness, "temporarily_unavailable",
        "Settings 必须保留可重试的临时网络故障细分状态"
    );
    assert!(
        settings.tts_provider.minimax_credential_configured,
        "临时网络故障后 credential ref 与 SQLite secret 仍必须标记为已配置"
    );
    assert!(
        cx.debug_bounds("voice-ready-banner").is_some(),
        "已配置 Voice 遭遇临时网络故障时仍应渲染日常 Ready 概览"
    );
    assert!(
        cx.debug_bounds("voice-setup-title").is_none(),
        "临时网络故障不得把用户送回 Voice Setup"
    );
    assert!(
        cx.debug_bounds("voice-setup-finish").is_none(),
        "临时网络故障不得要求用户重新完成配置"
    );
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("应找到可点击 selector：{selector}"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
