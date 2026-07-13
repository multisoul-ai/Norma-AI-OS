//! E2E-10 Voice 配置矩阵：通过真实 Settings 控件验证配置组合与持久化。

use app::ApiKeyDisplayState;
use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app_with_settings_store};

/// Voice 配置矩阵：模式、触发、麦克风、Provider、密钥与诊断设置必须互相独立并持久化。
///
/// 数据构造（含关键数值的推导过程）：
///   interaction modes = 2（Single Voice Command + Continuous Listening）
///   dictation triggers = 2（Hold + Toggle）
///   microphone choices = 3（System Default + 内置麦克风 + USB 麦克风）
///   provider profiles  = 2（Ollama + OpenAI-compatible）
///   ASR catalog entries = 2（Paraformer + Zipformer）
///   settings reloads    = 1（同一 SQLite store 创建第二个 App）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 fresh Voice Settings，检查安全缺省、模型元数据及不存在独立总开关
///   2. 注入两个可见麦克风，选择 USB 后再恢复 System Default，最后保留 USB
///   3. 选择 Toggle 与 Continuous Listening，展开 Agent/Reply 配置
///   4. 切到 OpenAI，编辑 Base URL、Model、API Key，开启 trace 并关闭 Agent 任务播报
///   5. 切回 Ollama 验证 profile 隔离，再切回 OpenAI 验证配置未丢失
///   6. 切回 Single Voice Command，关闭窗口；使用同一 SQLite store 创建新 App 并重新打开 Voice
///
/// 预期结果：
///   - 正断言：每种选择即时反映到稳定快照，重载后仍保留最终配置
///   - 正断言：API Key 只暴露 configured 状态，绝不渲染明文
///   - 负断言：Ollama 与 OpenAI profile 不得互相覆盖
///   - 负断言：两种模式都不得出现独立语音总开关
#[test]
fn e2e_10_voice_configuration_matrix_persists_across_app_reload() {
    let temp = tempfile::TempDir::new().expect("应创建隔离的 Voice Settings 数据目录");
    let store = storage::SettingsStore::new(temp.path().join("voice-matrix.sqlite"));
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Voice workspace");

    {
        let mut cx = TestAppContext::single();
        let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
        enter_canvas(&app, cx, workspace.path());
        app.update(cx, |app, cx| {
            app.set_voice_input_devices_for_e2e(
                vec![
                    (
                        "builtin-mic".to_string(),
                        "Built-in Microphone".to_string(),
                        true,
                    ),
                    (
                        "usb-mic".to_string(),
                        "Matrix USB Microphone".to_string(),
                        false,
                    ),
                ],
                cx,
            )
        });

        cx.simulate_keystrokes("cmd-,");
        click_debug(cx, "settings-section-voice");
        let defaults = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            defaults.voice_interaction_mode, "single_voice_command",
            "fresh 配置应默认 Single Voice Command"
        );
        assert_eq!(defaults.dictation_trigger, "hold", "fresh 配置应默认 Hold");
        assert_eq!(
            defaults.input_device_id, None,
            "fresh 配置应使用 System Default 麦克风"
        );
        assert!(
            !defaults.trace_enabled,
            "fresh 配置不应默认记录完整 prompt/response trace"
        );
        assert!(
            defaults.agent_task_announcements_enabled,
            "fresh 配置必须默认开启 Agent 任务终态播报"
        );
        assert!(
            defaults.voice_window_inset_enabled,
            "fresh 配置必须默认开启共享语音窗口内缘视觉"
        );
        assert_eq!(
            defaults.asr_models.len(),
            2,
            "首版受控模型库应精确展示两个模型"
        );
        assert_eq!(
            defaults.asr_models[0].display_name, "Paraformer Bilingual",
            "第一张模型卡应展示 Paraformer 的友好名称"
        );
        assert_eq!(
            defaults.asr_models[1].display_name, "Zipformer Streaming ZH",
            "第二张模型卡应展示 Zipformer Streaming ZH 的友好名称"
        );
        assert!(
            defaults.asr_models[1].streaming_partials,
            "Zipformer 卡片必须声明流式 partial 能力"
        );
        assert!(
            cx.debug_bounds("settings-voice-master-enable").is_none(),
            "新架构不得重新出现独立语音总开关"
        );
        assert!(
            cx.debug_bounds("settings-agent-provider-openai").is_some(),
            "Single Voice Command 页面也必须配置共享 Agent Provider"
        );

        click_debug(cx, "settings-voice-input-device-1");
        let usb = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            usb.input_device_id.as_deref(),
            Some("usb-mic"),
            "点击 USB 麦克风后应保存稳定 device id"
        );
        click_debug(cx, "settings-voice-input-device-default");
        let system_default = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            system_default.input_device_id, None,
            "System Default 应清除固定 device id"
        );
        click_debug(cx, "settings-voice-input-device-1");

        click_debug(cx, "settings-voice-trigger-toggle");
        click_debug(cx, "settings-voice-mode-continuous");
        scroll_debug(cx, "settings-content-scroll", -650.0);
        click_debug(cx, "settings-agent-provider-openai");
        scroll_debug(cx, "settings-content-scroll", -200.0);
        click_debug(cx, "settings-voice-advanced-toggle");
        replace_text(
            cx,
            "settings-voice-field-agent-base-url",
            "https://voice.example.test/v1",
        );
        scroll_debug(cx, "settings-content-scroll", 500.0);
        replace_text(cx, "settings-voice-field-agent-model", "voice-matrix-model");
        scroll_debug(cx, "settings-content-scroll", -500.0);
        replace_text(cx, "settings-voice-field-agent-api-key", "matrix-secret");
        click_debug(cx, "settings-voice-trace-toggle");
        click_debug(cx, "settings-agent-task-announcements-toggle");
        click_debug(cx, "settings-voice-window-inset-toggle");

        let openai = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            openai.agent_provider, "openai",
            "应切换为 OpenAI-compatible profile"
        );
        assert_eq!(
            openai.agent_base_url, "https://voice.example.test/v1",
            "OpenAI Base URL 应保存"
        );
        assert_eq!(
            openai.agent_model, "voice-matrix-model",
            "OpenAI Model 应保存"
        );
        assert!(
            openai.agent_api_key_configured,
            "OpenAI API Key 应保存为 configured"
        );
        assert_eq!(
            openai.agent_api_key_display_state,
            ApiKeyDisplayState::Configured,
            "API Key 提交后只能显示安全 configured 状态"
        );
        assert!(
            openai.trace_enabled,
            "点击 trace 开关后应启用完整诊断 trace"
        );
        assert!(
            !openai.agent_task_announcements_enabled,
            "点击 Agent 任务播报开关后应独立关闭播报"
        );
        assert!(
            !openai.voice_window_inset_enabled,
            "点击窗口内缘视觉开关后应独立关闭视觉但不改变其他 Voice 设置"
        );
        assert_eq!(
            openai.input_device_id.as_deref(),
            Some("usb-mic"),
            "Provider 修改不得清除麦克风"
        );

        scroll_debug(cx, "settings-content-scroll", 650.0);
        click_debug(cx, "settings-agent-provider-ollama");
        let ollama = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(ollama.agent_provider, "ollama", "应能切回 Ollama profile");
        assert_ne!(
            ollama.agent_model, "voice-matrix-model",
            "Ollama Model 不得被 OpenAI 覆盖"
        );
        assert_ne!(
            ollama.agent_base_url, "https://voice.example.test/v1",
            "Ollama URL 不得被 OpenAI 覆盖"
        );

        click_debug(cx, "settings-agent-provider-openai");
        click_debug(cx, "settings-voice-mode-dictation");
        let final_state = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            final_state.voice_interaction_mode, "single_voice_command",
            "最终模式应切回 Single Voice Command"
        );
        assert_eq!(
            final_state.dictation_trigger, "toggle",
            "模式切换不得重置 Toggle 偏好"
        );
        assert_eq!(
            final_state.selected_asr_model_id, None,
            "未安装选择模型时不得伪造选择"
        );
        cx.run_until_parked();
    }

    let mut reload_cx = TestAppContext::single();
    let (reloaded_app, reload_cx) =
        make_app_with_settings_store(&mut reload_cx, echo_factory(), store);
    enter_canvas(&reloaded_app, reload_cx, workspace.path());
    reload_cx.simulate_keystrokes("cmd-,");
    click_debug(reload_cx, "settings-section-voice");
    let reloaded = reloaded_app.update(reload_cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        reloaded.voice_interaction_mode, "single_voice_command",
        "重载后应保留最终 Single Voice Command"
    );
    assert_eq!(reloaded.dictation_trigger, "toggle", "重载后应保留 Toggle");
    assert_eq!(
        reloaded.input_device_id.as_deref(),
        Some("usb-mic"),
        "重载后应保留 USB device id"
    );
    assert_eq!(
        reloaded.agent_provider, "openai",
        "重载后应保留 OpenAI profile 选择"
    );
    assert_eq!(
        reloaded.agent_base_url, "https://voice.example.test/v1",
        "重载后应保留 OpenAI URL"
    );
    assert_eq!(
        reloaded.agent_model, "voice-matrix-model",
        "重载后应保留 OpenAI Model"
    );
    assert!(
        reloaded.agent_api_key_configured,
        "重载后 API Key 应保持 configured"
    );
    assert!(reloaded.trace_enabled, "重载后 trace 应保持启用");
    assert!(
        !reloaded.agent_task_announcements_enabled,
        "重载后 Agent 任务播报应保持关闭"
    );
    assert!(
        !reloaded.voice_window_inset_enabled,
        "重载后窗口内缘视觉应保持关闭"
    );
    assert_ne!(
        reloaded.agent_api_key_display_state,
        ApiKeyDisplayState::Unsafe,
        "重载后不得暴露密钥明文"
    );
}

fn replace_text(cx: &mut VisualTestContext, selector: &'static str, value: &str) {
    click_debug(cx, selector);
    cx.simulate_input(value);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    scroll_settings_target_into_view(cx, selector);
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应可点击"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn scroll_settings_target_into_view(cx: &mut VisualTestContext, selector: &'static str) {
    for _ in 0..3 {
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
        let margin = 24.0;
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
        scroll_debug(cx, "settings-content-scroll", delta);
    }
}

fn scroll_debug(cx: &mut VisualTestContext, selector: &'static str, delta_y: f32) {
    use gpui::{point, px, ScrollDelta, ScrollWheelEvent};
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` 应可滚动"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_event(ScrollWheelEvent {
        position: bounds.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(delta_y))),
        ..Default::default()
    });
    cx.run_until_parked();
}
