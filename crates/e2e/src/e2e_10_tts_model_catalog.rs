//! Local TTS 模型库的 Settings 用户可观察合约。

use crate::harness::{echo_factory, enter_canvas, make_app, make_app_with_settings_store};
use app::MultisoulApp;
use gpui::{
    point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
};

// 每次用户操作最多派生 4 层任务（点击→后台 worker→前台状态→持久化）;
// 8 次 run_until_parked 留出两轮余量，且不推进时间或依赖真实网络。
const E2E_SETTLE_PASSES: usize = 8;
// Voice Settings 最多进行三次滚动校正；每次至少推进一个可见目标高度，三次覆盖
// 上方、下方与布局重排后的补偿。24px 使目标不贴住裁剪边缘。
const SETTINGS_SCROLL_ATTEMPTS: usize = 3;
const SETTINGS_SCROLL_MARGIN_PX: f32 = 24.0;

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
    assert_eq!(snapshot.tts_models.len(), 2, "TTS 目录应只包含两个精选模型");
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
    assert_eq!(
        snapshot.tts_models[0].voices,
        vec!["default".to_string()],
        "单 Voice 的 Matcha 条目必须公开默认 Voice，而不能隐藏为模型文件细节"
    );
    assert_eq!(
        snapshot.tts_models[1].voices,
        vec!["af_heart".to_string(), "af_bella".to_string()],
        "多 Voice 的 Kokoro 条目必须公开两个可选择的目录 Voice"
    );
    assert_eq!(
        snapshot.tts_models[0].action, "install",
        "未安装模型必须只提供显式 Install，而不能误显示 Select、Delete 或 Cancel"
    );
    assert!(
        !snapshot.tts_models[0].deletable,
        "未安装模型不得暴露删除动作，因为没有可回收的受控目录"
    );
}

/// E2E-TTS-2：Voice Settings 的假模型运输必须覆盖安装、取消、选择、Voice 持久化、
/// 删除与隔离试听，而不依赖 GitHub、真实 ONNX 或声卡。
///
/// 数据构造（含关键数值的推导过程）：
///   catalog models         = 2（Matcha 单 Voice + Kokoro 双 Voice）
///   first install          = 1（无选择时自动选择 Matcha/default）
///   second install         = 1（已有选择时保持未选择）
///   Kokoro voices          = 2（af_heart 推荐 + af_bella）
///   persisted settings IDs = 2（模型 ID + Voice ID）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启用确定性假下载/假运行时校验，真实点击第一、第二个 Install；
///   2. 删除并重新安装未选中模型，再选择 Kokoro，验证默认 Voice 回退；
///   3. 试听 Bella 后切换到 Bella，检查试听代际失效、选择持久化及选中模型删除保护；
///   4. 删除未选中的 Matcha，注入进行中的假下载并点击 Cancel，确认没有 Ready 残留。
///
/// 预期结果：
///   - 断言 A：首装自动选中 default，后装不抢占已有选择；
///   - 断言 B：切换模型回退推荐 af_heart，切换 Voice 后持久化 af_bella；
///   - 断言 C：试听不驱动助手窗口内缘光，配置切换会使试听失效；
///   - 断言 D：选中模型无 Delete，取消下载后条目回到 Install 而非 Ready。
#[test]
fn e2e_tts_model_lifecycle_persists_voice_and_isolates_preview() {
    let mut cx = TestAppContext::single();
    let temp = tempfile::tempdir().expect("应创建隔离 TTS e2e 数据目录");
    let store = storage::SettingsStore::new(temp.path().join("settings.sqlite"));
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, false, cx)
    });
    enter_canvas(&app, cx, temp.path());
    cx.simulate_keystrokes("cmd-,");
    click(cx, "settings-section-voice");

    click(cx, "settings-tts-install-0");
    settle(cx);
    let first = snapshot(&app, cx);
    assert_eq!(
        first.tts_models[0].action, "selected",
        "首次成功安装时 Matcha 必须自动成为已选模型"
    );
    assert_eq!(
        first.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-baker"),
        "首次安装后必须保存 Matcha 的不可变模型 ID"
    );
    assert_eq!(
        first.selected_tts_voice_id.as_deref(),
        Some("default"),
        "单 Voice Matcha 首装后必须自动选择其 default Voice"
    );

    click(cx, "settings-tts-install-1");
    settle(cx);
    let second = snapshot(&app, cx);
    assert_eq!(
        second.tts_models[0].action, "selected",
        "安装第二个模型不得抢占首个已选模型"
    );
    assert_eq!(
        second.tts_models[1].action, "select",
        "已有选择时第二个成功安装的模型必须保持可显式 Select"
    );
    assert!(
        second.tts_models[1].deletable,
        "未选中的 Ready Kokoro 模型必须可删除以回收磁盘空间"
    );

    click(cx, "settings-tts-delete-1");
    settle(cx);
    let deleted = snapshot(&app, cx);
    assert_eq!(
        deleted.tts_models[1].action, "install",
        "删除未选中模型后条目必须回到 Install，而不是伪装成 Ready"
    );
    assert_eq!(
        deleted.selected_tts_model_id.as_deref(),
        Some("matcha-icefall-zh-baker"),
        "删除未选中 Kokoro 不得清除当前 Matcha 选择"
    );

    click(cx, "settings-tts-install-1");
    settle(cx);
    click(cx, "settings-tts-select-1");
    settle(cx);
    let switched = snapshot(&app, cx);
    assert_eq!(
        switched.selected_tts_model_id.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "显式 Select 后必须切换到 Kokoro 模型"
    );
    assert_eq!(
        switched.selected_tts_voice_id.as_deref(),
        Some("af_heart"),
        "旧 Matcha default 在 Kokoro 中不可用时必须回退到推荐 af_heart"
    );
    assert!(
        !switched.tts_models[1].deletable,
        "已选中的模型必须在用户可见状态投影中禁用 Delete，避免产生无效选择"
    );

    click(cx, "settings-tts-preview-1-1");
    settle(cx);
    let previewing = snapshot(&app, cx);
    let inset = app.update(cx, |app, _| app.voice_window_inset_e2e_snapshot());
    assert!(
        previewing.tts_preview_active,
        "点击 Voice Preview 后必须进入独立的试听活动状态"
    );
    assert!(
        !inset.active,
        "Voice Preview 不得触发助手回复专用的 voice-window inset"
    );
    assert_eq!(
        previewing.selected_tts_voice_id.as_deref(),
        Some("af_heart"),
        "试听 Bella 不得在用户确认前改写当前选择的 af_heart"
    );

    let preview_generation = previewing.tts_configuration_generation;
    click(cx, "settings-tts-voice-1-1");
    settle(cx);
    let bella = snapshot(&app, cx);
    assert_eq!(
        bella.selected_tts_voice_id.as_deref(),
        Some("af_bella"),
        "点击 Use 后必须切换并保存用户指定的 Bella Voice"
    );
    assert!(
        !bella.tts_preview_active,
        "模型或 Voice 配置变更必须取消旧试听，禁止与后续回复混音"
    );
    assert_ne!(
        bella.tts_configuration_generation, preview_generation,
        "Voice 配置变更必须推进 generation，使旧异步试听结果失效"
    );
    let persisted = MultisoulApp::load_tts_selection_for_e2e(store.clone());
    assert_eq!(
        persisted.0.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "后台持久化后 SQLite 必须保留 Kokoro 模型 ID"
    );
    assert_eq!(
        persisted.1.as_deref(),
        Some("af_bella"),
        "后台持久化后 SQLite 必须保留 Bella Voice ID"
    );

    click(cx, "settings-tts-delete-0");
    settle(cx);
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(true, true, cx)
    });
    click(cx, "settings-tts-install-0");
    settle(cx);
    let cancelling = snapshot(&app, cx);
    assert_eq!(
        cancelling.tts_models[0].action, "cancel",
        "下载进行中必须公开 Cancel 控件，而不能显示 Ready 或 Select"
    );
    click(cx, "settings-tts-cancel-0");
    settle(cx);
    app.update(cx, |app, cx| {
        app.set_fake_tts_transport_for_e2e(false, false, cx)
    });
    let cancelled = snapshot(&app, cx);
    assert_eq!(
        cancelled.tts_models[0].action, "install",
        "取消下载后模型必须回到 Install，不能遗留 Ready 清单或选择"
    );
    assert_eq!(
        cancelled.selected_tts_model_id.as_deref(),
        Some("kokoro-int8-multi-lang-v1_1"),
        "取消未选中模型的下载不得改变当前 Kokoro 选择"
    );
}

fn snapshot(
    app: &gpui::Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
) -> app::SettingsE2eSnapshot {
    app.update(cx, |app, cx| app.settings_e2e_snapshot(cx))
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..E2E_SETTLE_PASSES {
        cx.run_until_parked();
    }
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    scroll_settings_target_into_view(cx, selector);
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} 应可点击"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn scroll_settings_target_into_view(cx: &mut VisualTestContext, selector: &'static str) {
    for _ in 0..SETTINGS_SCROLL_ATTEMPTS {
        let Some(content) = cx.debug_bounds("settings-content-scroll") else {
            return;
        };
        let Some(target) = cx.debug_bounds(selector) else {
            return;
        };
        let content_left = f32::from(content.origin.x);
        let content_right = content_left + f32::from(content.size.width);
        let target_center_x = f32::from(target.center().x);
        if !(content_left..=content_right).contains(&target_center_x) {
            return;
        }
        let margin = SETTINGS_SCROLL_MARGIN_PX;
        let visible_top = f32::from(content.origin.y) + margin;
        let visible_bottom = f32::from(content.origin.y) + f32::from(content.size.height) - margin;
        let target_top = f32::from(target.origin.y);
        let target_bottom = target_top + f32::from(target.size.height);
        let delta = if target_top < visible_top {
            visible_top - target_top
        } else if target_bottom > visible_bottom {
            visible_bottom - target_bottom
        } else {
            return;
        };
        cx.simulate_event(ScrollWheelEvent {
            position: content.center(),
            delta: ScrollDelta::Pixels(point(px(0.0), px(delta))),
            ..Default::default()
        });
        cx.run_until_parked();
    }
}
