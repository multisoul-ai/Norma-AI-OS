//! Local TTS 模型库的 Settings 用户可观察合约。

use crate::harness::{echo_factory, enter_canvas, make_app};
use gpui::{Modifiers, TestAppContext, VisualTestContext};

/// E2E-TTS-1：Voice Settings 必须展示官方 TTS 下载、选择、删除与试听入口。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog entries = 2（Matcha + Kokoro）
///   initial selected = 0（新用户没有自动下载模型）
///   first entry actions = 1（Install）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. headless 打开 Canvas 与 Voice Settings；
///   2. 读取 TTS 目录投影并查询第一个模型的安装按钮；
///   3. 保持未安装状态，不发起真实网络下载。
///
/// 预期结果：
///   - 正断言：目录精确暴露两个官方模型与 Install 控件；
///   - 负断言：新用户不得已有选择或自动下载。
#[test]
fn e2e_tts_catalog_exposes_explicit_install_without_auto_selection() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::tempdir().expect("应创建 e2e workspace");
    enter_canvas(&app, cx, workspace.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");
    let snapshot = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(snapshot.tts_model_count, 2, "TTS 目录应只包含两个精选模型");
    assert_eq!(
        snapshot.selected_tts_model_id, None,
        "新用户不应自动选择或下载 TTS 模型"
    );
    assert!(
        cx.debug_bounds("settings-tts-install-0").is_some(),
        "第一个未安装模型应提供 Install 按钮"
    );
    assert!(
        cx.debug_bounds("settings-tts-model-1").is_some(),
        "第二个精选模型应出现在 Voice Settings"
    );
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} 应可点击"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
