/// Voice 页面锚点：Settings 应提供交互模式、共享 ASR 模型、输入设备、Agent 与高级配置入口。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings panel/content + voice state/provider/view 拆分源码
///   nav anchor = Voice
///   storage keys = mode + trigger + immutable ASR model ID + Agent task announcement toggle
///   input anchors = default / refresh / clear（覆盖默认输入、设备刷新、清除选择）
///   continuous anchors = Continuous Listening / Agent Provider / Local TTS
///   hidden debug env = MULTISOUL_VOICE_TRANSCRIPT，不应暴露成设置项
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 拼接 Settings 生产渲染源码
///   2. 检查 Voice 导航、分区标题、持久化 key、输入设备与单击编辑锚点
///   3. 检查 Edit 按钮和隐藏 transcript fallback 没有出现在 Settings UI
///
/// 预期结果：
///   - 正断言：Voice 页面包含新模式、模型库、Agent/TTS 与 Advanced
///   - 正断言：页面包含受控模型安装、输入设备、Agent 任务播报和 trace 锚点
///   - 负断言：不应保留旧模型目录、realtime 总开关或暴露 API Key 明文
#[test]
fn test_voice_settings_panel_contains_voice_configuration_anchors() {
    let source = concat!(
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/voice.rs"),
        include_str!("../../src/settings/voice/state.rs"),
        include_str!("../../src/settings/voice/state/model_selection.rs"),
        include_str!("../../src/settings/voice/state/provider_values.rs"),
        include_str!("../../src/settings/voice/state/values.rs"),
        include_str!("../../src/settings/voice/view.rs"),
        include_str!("../../src/settings/voice/view/editable.rs"),
        include_str!("../../src/settings/voice/view/editable/native.rs"),
        include_str!("../../src/settings/voice/view/interaction.rs"),
        include_str!("../../src/settings/voice/view/model_catalog.rs"),
        include_str!("../../src/settings/voice/view/tts_catalog.rs"),
        include_str!("../../src/settings/voice/view/spoken_reply.rs"),
        include_str!("../../src/settings/voice/view/provider.rs"),
        include_str!("../../../asr/src/catalog.rs")
    );
    macro_rules! assert_anchor {
        ($anchor:literal) => {
            assert!(
                source.contains($anchor),
                concat!("Voice 设置页应包含具体配置锚点: ", $anchor)
            );
        };
    }
    assert_anchor!("SettingsSection::Voice");
    assert_anchor!("Voice");
    assert_anchor!("Speech Recognition");
    assert_anchor!("Voice Interaction");
    assert_anchor!("Single Voice Command");
    assert_anchor!("Continuous Listening");
    assert_anchor!("Single Voice Command Trigger");
    assert_anchor!("Agent Provider");
    assert_anchor!("Spoken Reply");
    assert_anchor!("Local TTS");
    assert_anchor!("Advanced");
    assert_anchor!("voice.ollama.endpoint");
    assert_anchor!("voice.ollama.model");
    assert_anchor!("voice.dialogue.llm_mode");
    assert_anchor!("voice.dialogue.model");
    assert_anchor!("voice.refine.model");
    assert_anchor!("voice.interaction.mode");
    assert_anchor!("voice.single_voice_command.trigger");
    assert_anchor!("voice.asr.model_id");
    assert_anchor!("voice.input_device_id");
    assert_anchor!("voice.trace_enabled");
    assert_anchor!("voice.agent_task_announcements_enabled");
    assert_anchor!("voice.window_inset_enabled");
    assert_anchor!("settings-voice-mode-dictation");
    assert_anchor!("settings-voice-mode-continuous");
    assert_anchor!("settings-asr-model");
    assert_anchor!("Zipformer Streaming ZH");
    assert_anchor!("Zipformer Streaming CTC ZH");
    assert_anchor!("Recommendation pending");
    assert_anchor!("settings-voice-input-device-default");
    assert_anchor!("settings-voice-input-device-refresh");
    assert_anchor!("settings-voice-input-device-clear");
    assert_anchor!("agent.provider.active");
    assert_anchor!("agent.provider.ollama.base_url");
    assert_anchor!("agent.provider.openai.base_url");
    assert_anchor!("settings-agent-provider-ollama");
    assert_anchor!("settings-agent-provider-openai");
    assert_anchor!("settings-voice-field-");
    assert_anchor!("agent_base_url");
    assert_anchor!("agent_model");
    assert_anchor!("agent_api_key");
    assert_anchor!("settings-agent-provider-api-key-clear");
    assert_anchor!("Base URL");
    assert_anchor!("API Key");
    assert_anchor!("settings-voice-trace-toggle");
    assert_anchor!("settings-agent-task-announcements-toggle");
    assert_anchor!("Agent task announcements");
    assert_anchor!("settings-voice-window-inset-toggle");
    assert_anchor!("Voice-driven window inset");
    assert_anchor!("on_mouse_down");
    assert_anchor!("begin_edit");
    assert!(
        !source.contains("MULTISOUL_VOICE_TRANSCRIPT"),
        "Voice Settings 不应暴露测试用 transcript 注入环境变量"
    );
    assert!(
        !source.contains("top-secret"),
        "Settings 源码不得包含测试 API Key 明文"
    );
    assert!(
        !source.contains("voice.paraformer.model_dir"),
        "新 Voice Settings 不得继续读取旧 Paraformer 路径 key"
    );
    assert!(
        !source.contains("voice.realtime.enabled"),
        "新 Voice Settings 不得保留独立 Always Listening 总开关"
    );
    assert!(
        !source.contains("settings-voice-edit")
            && !source.contains("settings-agent-provider-api-key-edit"),
        "Voice Settings 不应保留 Edit 或 Replace 按钮锚点"
    );
}
