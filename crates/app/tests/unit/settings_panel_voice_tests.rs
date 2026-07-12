/// Voice 页面锚点：Settings 应提供语音识别、输入设备、规划模型、Always Listening 和 trace 配置入口。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings panel/content + voice state/provider/view 拆分源码
///   nav anchor = Voice
///   storage keys = voice.* key（one-shot voice + Always Listening toggle）
///   input anchors = default / refresh / clear（覆盖默认输入、设备刷新、清除选择）
///   always listening anchors = Always Listening / Enable Always Listening
///   hidden debug env = MULTISOUL_VOICE_TRANSCRIPT，不应暴露成设置项
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 拼接 Settings 生产渲染源码
///   2. 检查 Voice 导航、分区标题、持久化 key、输入设备与单击编辑锚点
///   3. 检查 Edit 按钮和隐藏 transcript fallback 没有出现在 Settings UI
///
/// 预期结果：
///   - 正断言：Voice 页面包含 Agent Provider / Speech Recognition / Always Listening / Advanced
///   - 正断言：页面包含模型目录选择、输入设备、恢复默认和 trace toggle 锚点
///   - 负断言：不应保留 Edit/Replace 按钮，也不暴露测试 transcript 或 API Key 明文
#[test]
fn test_voice_settings_panel_contains_voice_configuration_anchors() {
    let source = format!(
        "{}{}{}{}{}{}{}{}{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/voice.rs"),
        include_str!("../../src/settings/voice/state.rs"),
        include_str!("../../src/settings/voice/state/provider_values.rs"),
        include_str!("../../src/settings/voice/state/values.rs"),
        include_str!("../../src/settings/voice/view.rs"),
        include_str!("../../src/settings/voice/view/editable.rs"),
        include_str!("../../src/settings/voice/view/editable/native.rs"),
        include_str!("../../src/settings/voice/view/always_listening.rs"),
        include_str!("../../src/settings/voice/view/provider.rs")
    );
    let anchors = [
        "SettingsSection::Voice",
        "Voice",
        "Speech Recognition",
        "Agent Provider",
        "Always Listening",
        "Enable Always Listening",
        "Advanced",
        "voice.ollama.endpoint",
        "voice.ollama.model",
        "voice.dialogue.llm_mode",
        "voice.dialogue.model",
        "voice.refine.model",
        "voice.paraformer.model_dir",
        "voice.input_device_id",
        "voice.trace_enabled",
        "voice.realtime.enabled",
        "settings-voice-model-dir-choose",
        "settings-voice-model-dir-default",
        "settings-voice-input-device-default",
        "settings-voice-input-device-refresh",
        "settings-voice-input-device-clear",
        "settings-voice-realtime-toggle",
        "agent.provider.active",
        "agent.provider.ollama.base_url",
        "agent.provider.openai.base_url",
        "settings-agent-provider-ollama",
        "settings-agent-provider-openai",
        "settings-voice-field-",
        "agent_base_url",
        "agent_model",
        "agent_api_key",
        "settings-agent-provider-api-key-clear",
        "Base URL",
        "API Key",
        "settings-voice-trace-toggle",
        "on_mouse_down",
        "begin_edit",
    ];

    for anchor in anchors {
        assert!(
            source.contains(anchor),
            "Voice 设置页应包含配置锚点: {anchor}"
        );
    }
    assert!(
        !source.contains("MULTISOUL_VOICE_TRANSCRIPT"),
        "Voice Settings 不应暴露测试用 transcript 注入环境变量"
    );
    assert!(
        !source.contains("top-secret"),
        "Settings 源码不得包含测试 API Key 明文"
    );
    assert!(
        !source.contains("settings-voice-edit")
            && !source.contains("settings-agent-provider-api-key-edit"),
        "Voice Settings 不应保留 Edit 或 Replace 按钮锚点"
    );
}
