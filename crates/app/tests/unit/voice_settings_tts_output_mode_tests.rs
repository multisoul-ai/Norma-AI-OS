use super::super::*;
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState};

fn local_kokoro_state() -> (VoiceSettingsState, tts::TtsVoiceId) {
    let voice = tts::TtsVoiceId::parse("af_bella").expect("Bella Voice ID 必须有效");
    let state = VoiceSettingsState::from_sources(
        SavedVoiceSettings {
            tts_model_id: Some(
                tts::TtsModelId::parse(tts::KOKORO_MULTI_LANG_MODEL_ID)
                    .expect("Kokoro model ID 必须有效"),
            ),
            tts_voice_id: Some(voice.clone()),
            active_tts_provider_id: Some(
                tts::TtsProviderId::parse(tts::LOCAL_TTS_PROVIDER_ID)
                    .expect("本机 Provider ID 必须有效"),
            ),
            ..SavedVoiceSettings::default()
        },
        EnvVoiceSettings::default(),
    );
    (state, voice)
}

/// TTS-OUTPUT-1：关闭只是草稿，取消后本机音色保持生效。
///
/// 数据构造：当前生效 = Kokoro + af_bella + local。草稿切到 off，不点应用。
///
/// 执行过程：打开选择器，选关闭，再取消。
///
/// 预期结果：
///   - 正断言：取消后 Provider 仍是 local，音色仍是 Bella。
///   - 负断言：选择器关闭，不得留下 off。
#[test]
fn test_off_draft_cancel_keeps_the_active_local_voice() {
    let (mut state, voice) = local_kokoro_state();
    state.open_tts_picker();
    state.select_tts_picker_provider(tts::OFF_TTS_PROVIDER_ID);

    assert_eq!(
        state.tts_picker_provider_id(),
        Some(tts::OFF_TTS_PROVIDER_ID),
        "草稿里应能选中关闭"
    );

    state.cancel_tts_picker();

    assert!(!state.tts_picker_open(), "取消后选择器必须关闭");
    assert_eq!(
        state
            .values()
            .active_tts_provider_id()
            .map(tts::TtsProviderId::as_str),
        Some(tts::LOCAL_TTS_PROVIDER_ID),
        "取消关闭草稿不得改生效 Provider"
    );
    assert_eq!(
        state.values().tts_voice_id(),
        Some(&voice),
        "取消关闭草稿不得改本机 Bella"
    );
}

/// TTS-OUTPUT-2：提交关闭后回复只走文字，本机音色配置保留，语音就绪不再被挡住。
///
/// 数据构造：当前生效 = Kokoro + af_bella。草稿选 off 后提交。
///
/// 执行过程：打开选择器，选关闭，提交输出模式，再读 Provider、音色和 SpokenReplies。
///
/// 预期结果：
///   - 正断言：生效 Provider 为 off，SpokenReplies 为 Ready。
///   - 负断言：本机 model/voice 不变，选择器不再打开。
#[test]
fn test_off_commit_is_ready_for_text_and_keeps_the_local_voice() {
    let (mut state, voice) = local_kokoro_state();
    let model = state.values().tts_model_id().cloned();
    state.open_tts_picker();
    state.select_tts_picker_provider(tts::OFF_TTS_PROVIDER_ID);

    assert!(state.tts_picker_ready(), "关闭不需要音色，草稿即可应用");
    assert!(state.commit_tts_picker_output_mode(), "关闭草稿应能提交");

    assert_eq!(
        state
            .values()
            .active_tts_provider_id()
            .map(tts::TtsProviderId::as_str),
        Some(tts::OFF_TTS_PROVIDER_ID),
        "提交后生效 Provider 必须是 off"
    );
    assert_eq!(
        state.requirement_state(VoiceRequirement::SpokenReplies),
        VoiceRequirementState::Ready,
        "显式关闭表示文字回复已就绪，不能再挡语音设置"
    );
    assert_eq!(
        state.values().tts_voice_id(),
        Some(&voice),
        "关闭不得清掉已保存的本机音色"
    );
    assert_eq!(
        state.values().tts_model_id().cloned(),
        model,
        "关闭不得清掉已保存的本机模型"
    );
    assert!(!state.tts_picker_open(), "提交后选择器必须关闭");
}

/// TTS-OUTPUT-3：系统旁白只能提交已枚举的 macOS 声音，且不覆盖本机音色。
///
/// 数据构造：当前生效 = Kokoro + af_bella。枚举名单只有 Tingting 与 Bad News。
/// 未入名单的 Samantha 不得写入草稿。
///
/// 执行过程：
///   1. 未注入名单时选择系统旁白并尝试提交；
///   2. 注入两个人声，拒绝名单外声音；
///   3. 选中 Bad News 后提交。
///
/// 预期结果：
///   - 正断言：提交后 Provider 为 system，声音名为 Bad News，SpokenReplies 为 Ready。
///   - 负断言：名单外声音不能选中；本机 Bella 保持不变。
#[test]
fn test_system_voice_commits_only_an_enumerated_macos_voice() {
    let (mut state, local_voice) = local_kokoro_state();
    state.open_tts_picker();
    state.select_tts_picker_provider(tts::SYSTEM_TTS_PROVIDER_ID);

    assert!(
        !state.tts_picker_ready(),
        "还没有 macOS 声音名单时系统旁白不得应用"
    );
    assert!(
        !state.commit_tts_picker_output_mode(),
        "空名单不得提交系统旁白"
    );
    assert_eq!(
        state
            .values()
            .active_tts_provider_id()
            .map(tts::TtsProviderId::as_str),
        Some(tts::LOCAL_TTS_PROVIDER_ID),
        "失败的系统旁白提交不得改变本机 Provider"
    );

    state.set_system_voice_names(vec!["Tingting".to_string(), "Bad News".to_string()]);
    assert!(
        !state.set_tts_picker_system_voice("Samantha"),
        "名单外的 Samantha 不得进入草稿"
    );
    assert!(
        state.set_tts_picker_system_voice("Bad News"),
        "名单内带空格的声音名必须可以选中"
    );
    assert!(state.tts_picker_ready(), "选中已枚举声音后系统旁白才可应用");
    assert!(
        state.commit_tts_picker_output_mode(),
        "已枚举的系统声音应能提交"
    );

    assert_eq!(
        state
            .values()
            .active_tts_provider_id()
            .map(tts::TtsProviderId::as_str),
        Some(tts::SYSTEM_TTS_PROVIDER_ID),
        "提交后生效 Provider 必须是 system"
    );
    assert_eq!(
        state
            .values()
            .system_tts_voice()
            .map(tts::TtsVoiceId::as_str),
        Some("Bad News"),
        "提交后必须记下系统声音名"
    );
    assert_eq!(
        state.values().tts_voice_id(),
        Some(&local_voice),
        "系统声音不得覆盖本机 Bella"
    );
    assert_eq!(
        state.requirement_state(VoiceRequirement::SpokenReplies),
        VoiceRequirementState::Ready,
        "已选定的系统旁白应视为语音输出就绪"
    );
}
