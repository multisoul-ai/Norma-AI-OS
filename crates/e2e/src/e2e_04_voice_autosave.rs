//! E2E-04：Voice Provider 文本框自动保存覆盖。
//!
//! 本测试从渲染树点击真实 Settings 控件，并通过 GPUI 平台文本输入接口写入内容；
//! 不调用 Voice state、提交 helper，也不发送 Enter，以锁定用户看到的自动保存语义。

use std::time::Duration;

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app_with_settings_store};

/// Model 与 Base URL 输入后不按 Enter，重新加载 SettingsStore 仍应读回新值。
///
/// 场景：用户打开 Voice Settings，修改 Model 后直接切换到 Base URL 并继续修改，两个
/// 字段都不按 Enter；随后关闭当前 App 上下文并从同一个临时 SQLite 重新加载 Voice 设置。
/// 数据构造：Model=`autosaved-model`、Base URL=`https://autosave.example.test/v1`；两者
/// 均与 Ollama 默认值不同，SettingsStore 指向测试临时目录中的 `app.sqlite`。
/// 执行过程：两个字段都通过真实 debug hitbox 与 `simulate_input` 输入；先将 mock
/// clock 推进 299ms 并确认数据库仍是旧 Base URL，再推进 1ms 达到产品 300ms 合并窗口，
/// 运行后台任务至停驻后从同一个 SettingsStore 重新加载 Provider 值。
/// 预期结果：输入期间仍停留在 Base URL 编辑态，证明未依赖 Enter；重新加载后两个值
/// 均精确等于新内容，且不等于默认值，证明后台 SQLite 自动保存形成完整闭环。
#[test]
fn e2e_04_voice_provider_text_auto_saves_without_enter_and_reloads() {
    let data = tempfile::TempDir::new().expect("测试应创建临时 settings 目录");
    let store = storage::SettingsStore::new(data.path().join("app.sqlite"));

    let (original_model, original_base_url) = {
        let mut cx = TestAppContext::single();
        let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
        let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
        enter_canvas(&app, cx, workspace.path());

        click_debug(cx, "toolbar-settings");
        click_debug(cx, "settings-section-voice");
        let before = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_ne!(
            before.agent_model, "autosaved-model",
            "测试前置 Model 必须与新值不同，避免环境配置造成假绿"
        );
        assert_ne!(
            before.agent_base_url, "https://autosave.example.test/v1",
            "测试前置 Base URL 必须与新值不同，避免环境配置造成假绿"
        );
        click_debug(cx, "settings-voice-field-agent-model");
        cx.simulate_input("autosaved-model");
        click_debug(cx, "settings-voice-field-agent-base-url");
        cx.simulate_input("https://autosave.example.test/v1");
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(299));
        cx.run_until_parked();
        let (before_delay_base_url, _before_delay_model, _before_delay_key) =
            app::MultisoulApp::load_voice_provider_settings_for_e2e(store.clone());
        assert_eq!(
            before_delay_base_url, before.agent_base_url,
            "299ms 时 Base URL 不应提前写入 SQLite"
        );
        assert_ne!(
            before_delay_base_url, "https://autosave.example.test/v1",
            "299ms 时数据库不得已经包含待合并的新 Base URL"
        );
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(1));
        cx.run_until_parked();

        let editing = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            editing.voice_editing_field,
            Some("agent_base_url"),
            "自动保存不应要求 Enter 或退出当前字段，snapshot: {:?}",
            editing
        );
        (before.agent_model, before.agent_base_url)
    };

    let (saved_base_url, saved_model, _api_key_configured) =
        app::MultisoulApp::load_voice_provider_settings_for_e2e(store);
    assert_eq!(
        saved_model, "autosaved-model",
        "重新加载后应从 SQLite 读回自动保存的 Model"
    );
    assert_ne!(
        saved_model, original_model,
        "重新加载后的 Model 不得回退到输入前旧值"
    );
    assert_eq!(
        saved_base_url, "https://autosave.example.test/v1",
        "重新加载后应从 SQLite 读回自动保存的 Base URL"
    );
    assert_ne!(
        saved_base_url, original_base_url,
        "重新加载后的 Base URL 不得回退到输入前旧值"
    );
}

/// API Key 输入后失焦，应安全显示已配置状态并能从 SQLite 重新加载。
///
/// 场景：用户打开 Voice Settings，在 API Key 真实文本框中输入密钥，不按 Enter，随后
/// 点击 Model 文本框令 API Key 失焦；界面不得把已保存密钥重新显示成未配置。
/// 数据构造：API Key=`sk-e2e-autosave`，SettingsStore 指向测试临时目录中的
/// `app.sqlite`，初始状态明确没有 API Key，避免环境设置造成假绿。
/// 执行过程：通过真实 debug hitbox 与 `simulate_input` 输入密钥，不按 Enter，把 mock
/// clock 推进 300ms 验证定时写库，再点击 Model；销毁首个 App 后从同一 store 新建 App。
/// 预期结果：内存、SQLite 与重建 App 都标记 API Key 已配置；失焦和重载后的控件只
/// 显示安全的 configured 状态，不得显示未配置，也不得把任何渲染原文暴露给快照。
#[test]
fn e2e_04_voice_api_key_auto_save_shows_configured_after_blur_and_reload() {
    let data = tempfile::TempDir::new().expect("测试应创建临时 settings 目录");
    let store = storage::SettingsStore::new(data.path().join("app.sqlite"));

    let after_blur = {
        let mut cx = TestAppContext::single();
        let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
        let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
        enter_canvas(&app, cx, workspace.path());

        click_debug(cx, "toolbar-settings");
        click_debug(cx, "settings-section-voice");
        let before = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert!(
            !before.agent_api_key_configured,
            "测试前置状态不得已经配置 API Key，snapshot: {:?}",
            before
        );

        click_debug(cx, "settings-voice-field-agent-api-key");
        cx.simulate_input("sk-e2e-autosave");
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(300));
        cx.run_until_parked();
        let (_base_url, _model, timer_saved_api_key) =
            app::MultisoulApp::load_voice_provider_settings_for_e2e(store.clone());
        assert!(
            timer_saved_api_key,
            "API Key 保持聚焦且不按 Enter 时，300ms 后也应自动写入 SQLite"
        );
        click_debug(cx, "settings-voice-field-agent-model");
        cx.run_until_parked();
        app.update(cx, |app, cx| app.settings_e2e_snapshot(cx))
    };
    assert!(
        after_blur.agent_api_key_configured,
        "API Key 失焦后内存状态仍应标记为已配置，snapshot: {:?}",
        after_blur
    );
    let (_saved_base_url, _saved_model, saved_api_key_configured) =
        app::MultisoulApp::load_voice_provider_settings_for_e2e(store.clone());
    assert!(
        saved_api_key_configured,
        "从同一个 SQLite 重新加载后 API Key 仍应标记为已配置"
    );
    assert_eq!(
        after_blur.agent_api_key_display_state,
        app::ApiKeyDisplayState::Configured,
        "API Key 失焦后真实输入控件应显示安全的已配置提示"
    );
    assert_ne!(
        after_blur.agent_api_key_display_state,
        app::ApiKeyDisplayState::NotConfigured,
        "已经配置的 API Key 不得重新显示为未配置"
    );
    assert_ne!(
        after_blur.agent_api_key_display_state,
        app::ApiKeyDisplayState::Unsafe,
        "API Key 失焦后不得在渲染文本中暴露明文"
    );

    let reloaded = {
        let mut cx = TestAppContext::single();
        let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store);
        let workspace = tempfile::TempDir::new().expect("重载验证应创建临时 workspace");
        enter_canvas(&app, cx, workspace.path());
        click_debug(cx, "toolbar-settings");
        click_debug(cx, "settings-section-voice");
        app.update(cx, |app, cx| app.settings_e2e_snapshot(cx))
    };
    assert!(
        reloaded.agent_api_key_configured,
        "重建 App 后 API Key 仍应从 SQLite 加载为 configured，snapshot: {:?}",
        reloaded
    );
    assert_eq!(
        reloaded.agent_api_key_display_state,
        app::ApiKeyDisplayState::Configured,
        "重建 App 后输入控件仍应显示安全的 configured 状态"
    );
}

/// Tab 聚焦下一个真实文本框后，输入与自动保存必须切换到新的字段。
///
/// 场景：用户点击 Base URL 后按 Tab 进入 Model，不再用鼠标点击 Model，随后直接输入；
/// 焦点移动不能只改变 GPUI 输入目标而让 Voice state 仍停留在 Base URL 编辑态。
/// 数据构造：Model 输入为 `tab-autosaved-model`，临时 SettingsStore 隔离本机配置。
/// 执行过程：点击 Base URL、发送真实 Tab 键、通过平台输入接口写入 Model，不按 Enter，
/// 推进 300ms 自动保存窗口，再读取 App snapshot 与同一 SQLite。
/// 预期结果：编辑字段为 Model，当前值和 SQLite 重载值均精确等于新 Model；不得把输入
/// 静默丢弃、追加到旧值或仍报告正在编辑 Base URL。
#[test]
fn e2e_04_voice_text_inputs_auto_save_when_reached_by_tab() {
    let data = tempfile::TempDir::new().expect("测试应创建临时 settings 目录");
    let store = storage::SettingsStore::new(data.path().join("app.sqlite"));
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");

    click_debug(cx, "settings-voice-field-agent-base-url");
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    cx.simulate_input("tab-autosaved-model");
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(300));
    cx.run_until_parked();

    let current = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        current.voice_editing_field,
        Some("agent_model"),
        "Tab 聚焦后 Voice state 应切换到 Model 编辑态，snapshot: {:?}",
        current
    );
    assert_eq!(
        current.agent_model, "tab-autosaved-model",
        "Tab 到 Model 后平台输入应精确替换并更新当前配置"
    );
    let (_base_url, saved_model, _configured) =
        app::MultisoulApp::load_voice_provider_settings_for_e2e(store);
    assert_eq!(
        saved_model, "tab-autosaved-model",
        "Tab 到 Model 后不按 Enter，300ms 后 SQLite 也应自动保存新值"
    );
}

/// 关闭再打开 Settings 后，API Key 输入实体不得继续持有本次输入的可编辑内容。
///
/// 场景：用户输入 API Key 后不按 Enter，也不切换到另一个文本框，直接点击 Settings
/// 关闭按钮；稍后重新打开 Voice Settings。
/// 数据构造：API Key 使用测试值，SettingsStore 位于临时目录；快照只暴露安全枚举。
/// 执行过程：真实点击 API Key 并平台输入，推进 300ms 自动保存，点击关闭，再重新打开
/// Voice section，检查编辑态、控件安全显示状态和 SQLite 重载状态。
/// 预期结果：关闭时应结束编辑并清理输入实体；重新打开只显示 configured 固定标记，
/// 不得仍处于编辑态、按输入长度显示 MaskedEditing，或丢失已保存配置。
#[test]
fn e2e_04_voice_api_key_close_reopen_clears_editor_but_keeps_saved_value() {
    let data = tempfile::TempDir::new().expect("测试应创建临时 settings 目录");
    let store = storage::SettingsStore::new(data.path().join("app.sqlite"));
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");

    click_debug(cx, "settings-voice-field-agent-api-key");
    cx.simulate_input("close-reopen-test-key");
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    click_debug(cx, "settings-close");
    let closed = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(!closed.settings_open, "点击关闭后 Settings 面板应关闭");

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");
    let reopened = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        reopened.voice_editing_field, None,
        "关闭再打开后不得残留 API Key 编辑态，snapshot: {:?}",
        reopened
    );
    assert!(
        reopened.agent_api_key_configured,
        "关闭再打开后当前 Provider 仍应标记 API Key configured"
    );
    assert_eq!(
        reopened.agent_api_key_display_state,
        app::ApiKeyDisplayState::Configured,
        "关闭再打开后 API Key 控件只应显示固定 configured 状态"
    );
    assert_ne!(
        reopened.agent_api_key_display_state,
        app::ApiKeyDisplayState::MaskedEditing,
        "关闭再打开后不得保留按输入长度渲染的编辑内容"
    );
    let (_base_url, _model, reloaded_configured) =
        app::MultisoulApp::load_voice_provider_settings_for_e2e(store);
    assert!(
        reloaded_configured,
        "关闭再打开后 SQLite 重载仍应保留已自动保存的 API Key"
    );
}

/// Clear 后应真正离开 API Key 编辑焦点，并允许单击同一输入框重新配置。
///
/// 场景：用户输入并自动保存 API Key，保持该输入框聚焦时点击 Clear，随后立即再次点击
/// 同一输入框输入替换值；不能因为输入框仍持有旧焦点而让第二次输入被状态层静默忽略。
/// 数据构造：两次 API Key 都是测试值，临时 SettingsStore 隔离本机配置。
/// 执行过程：真实输入首个值并等待 300ms，点击真实 Clear，验证未配置；再点击 API Key、
/// 输入替换值并等待 300ms，读取 App 与 SQLite configured 状态。
/// 预期结果：Clear 后编辑态结束且显示未配置；再次单击后能重新进入编辑并自动保存，
/// 当前状态与 SQLite 重载均为 configured。
#[test]
fn e2e_04_voice_api_key_can_be_reentered_immediately_after_clear() {
    let data = tempfile::TempDir::new().expect("测试应创建临时 settings 目录");
    let store = storage::SettingsStore::new(data.path().join("app.sqlite"));
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
    let workspace = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-voice");

    click_debug(cx, "settings-voice-field-agent-api-key");
    cx.simulate_input("before-clear-test-key");
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    click_debug(cx, "settings-agent-provider-api-key-clear");
    let cleared = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        !cleared.agent_api_key_configured,
        "明确 Clear 后当前 Provider 应立即变为未配置"
    );
    assert_eq!(
        cleared.voice_editing_field, None,
        "明确 Clear 后 API Key 编辑态应结束"
    );
    assert_eq!(
        cleared.agent_api_key_display_state,
        app::ApiKeyDisplayState::NotConfigured,
        "明确 Clear 后输入框应显示未配置"
    );

    click_debug(cx, "settings-voice-field-agent-api-key");
    cx.simulate_input("after-clear-test-key");
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(300));
    cx.run_until_parked();
    let reconfigured = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        reconfigured.agent_api_key_configured,
        "Clear 后再次单击输入框，应能重新自动保存 API Key"
    );
    let (_base_url, _model, reloaded_configured) =
        app::MultisoulApp::load_voice_provider_settings_for_e2e(store);
    assert!(
        reloaded_configured,
        "Clear 后重新输入的 API Key 应能从 SQLite 重载为 configured"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 在点击前应存在 debug bounds"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}
