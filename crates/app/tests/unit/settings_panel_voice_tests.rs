/// Voice 页面锚点：Settings 应提供语音识别、输入设备、规划模型、Always Listening 和 trace 配置入口。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings_panel.rs + settings_content.rs + settings_voice.rs
///            + voice state/view/realtime 拆分源码
///   nav anchor = Voice
///   storage keys = 11 个 voice.* key（6 个 one-shot voice key + 5 个 realtime key）
///   input anchors = default / refresh / clear（覆盖默认输入、设备刷新、清除选择）
///   realtime anchors = Realtime conversation / Enable Always Listening / OpenAI API Key
///                      / Realtime Model / Card Planner Model / Sidecar Path
///   hidden debug env = MULTISOUL_VOICE_TRANSCRIPT，不应暴露成设置项
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 拼接 Settings 生产渲染源码
///   2. 检查 Voice 导航、分区标题、持久化 key、输入设备锚点、realtime 锚点与按钮锚点
///   3. 检查隐藏 transcript fallback 没有出现在 Settings UI
///
/// 预期结果：
///   - 正断言：Voice 页面包含 Speech Recognition / Command Planning / Realtime conversation / Advanced
///   - 正断言：页面包含模型目录选择、输入设备、恢复默认和 trace toggle 锚点
///   - 负断言：不应暴露 MULTISOUL_VOICE_TRANSCRIPT
#[test]
fn test_voice_settings_panel_contains_voice_configuration_anchors() {
    let source = format!(
        "{}{}{}{}{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/voice.rs"),
        include_str!("../../src/settings/voice/state.rs"),
        include_str!("../../src/settings/voice/state/values.rs"),
        include_str!("../../src/settings/voice/view.rs"),
        include_str!("../../src/settings/voice/view/realtime.rs")
    );
    let anchors = [
        "SettingsSection::Voice",
        "Voice",
        "Speech Recognition",
        "Command Planning",
        "Realtime conversation",
        "Enable Always Listening",
        "OpenAI API Key",
        "Realtime Model",
        "Card Planner Model",
        "Sidecar Path",
        "Advanced",
        "voice.ollama.endpoint",
        "voice.ollama.model",
        "voice.refine.model",
        "voice.paraformer.model_dir",
        "voice.input_device_id",
        "voice.trace_enabled",
        "voice.realtime.enabled",
        "voice.openai.api_key",
        "voice.openai.realtime_model",
        "voice.openai.card_planner_model",
        "voice.openai.sidecar_path",
        "settings-voice-model-dir-choose",
        "settings-voice-model-dir-default",
        "settings-voice-input-device-default",
        "settings-voice-input-device-refresh",
        "settings-voice-input-device-clear",
        "settings-voice-realtime-toggle",
        "settings-voice-trace-toggle",
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
}
