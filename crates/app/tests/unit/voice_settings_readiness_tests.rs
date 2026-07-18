use super::super::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsState};
use crate::settings::voice::{VoiceEntryState, VoiceRequirement, VoiceRequirementState};

/// VoiceSettingsState 汇总四项真实配置：仅当麦克风、已安装 ASR、已验证 Provider、已安装 TTS 音色都可用时才 Ready
///
/// 数据构造（含关键数值的推导过程）：
///   必需项总数 = 4
///   初始 Ready 项数 = 0
///   最终 Ready 项数 = 4
///   推荐 ASR = Zipformer Bilingual
///   推荐 TTS = Matcha Chinese-English + Default Bilingual Voice
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从空 saved/env 创建 state，确认首先缺麦克风
///   2. 将麦克风和 Provider 标记 Ready
///   3. 将推荐 ASR / TTS 标记安装完成并选中默认音色
///   4. 重新计算 readiness
///
/// 预期结果：
///   - 正断言：初始状态为 SetUp(Microphone)，最终状态为 Ready
///   - 负断言：TTS 未配置前不得提前 Ready
#[test]
fn settings_state_requires_all_four_runtime_capabilities() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );

    assert_eq!(
        state.readiness(),
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::Microphone,
        },
        "fresh settings state must start setup at microphone permission"
    );

    state.set_microphone_requirement(VoiceRequirementState::Ready);
    state.set_provider_requirement(VoiceRequirementState::Ready);
    let asr_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
        .expect("built-in Zipformer model id should parse");
    state.model_manager_mut().mark_ready(&asr_id);
    state.select_asr_model(Some(asr_id));

    assert_ne!(
        state.readiness(),
        VoiceEntryState::Ready,
        "Voice must remain unavailable until required TTS model and voice are selected"
    );

    let tts_id = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID)
        .expect("built-in Matcha model id should parse");
    let voice_id =
        tts::TtsVoiceId::parse("default").expect("built-in bilingual voice id should parse");
    state.tts_model_manager_mut().mark_ready(&tts_id);
    state.select_tts_model_and_voice(Some(tts_id), Some(voice_id));

    assert_eq!(
        state.readiness(),
        VoiceEntryState::Ready,
        "all four usable capabilities must make Voice ready"
    );
    assert_ne!(
        state.readiness(),
        VoiceEntryState::SetUp {
            first_incomplete: VoiceRequirement::SpokenReplies,
        },
        "selected ready TTS must clear the Spoken Replies setup gate"
    );
}

/// 推荐模型包分阶段完成：已完成 ASR 的字节仍必须计入 TTS 下载期间的总进度
///
/// 数据构造（含关键数值的推导过程）：
///   ASR total = Zipformer catalog archive_bytes
///   ASR completed = ASR total（状态 Ready）
///   TTS total = Matcha catalog download_bytes
///   TTS completed = TTS total / 2
///   expected% = (ASR total + TTS total / 2) * 100 / (ASR total + TTS total)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 选中推荐 ASR、TTS 和默认音色
///   2. 将 ASR 标记 Ready，将 TTS 标记为下载一半
///   3. 从完整推荐包固定总字节计算入口进度
///
/// 预期结果：
///   - 正断言：SettingUp 百分比包含已经完成的 ASR 字节
///   - 负断言：不得退化成只看剩余 TTS 下载的 50%
#[test]
fn completed_asr_bytes_remain_in_recommended_package_progress() {
    let mut state = VoiceSettingsState::from_sources(
        SavedVoiceSettings::default(),
        EnvVoiceSettings::default(),
    );
    state.set_microphone_requirement(VoiceRequirementState::Ready);
    state.set_provider_requirement(VoiceRequirementState::Ready);
    let asr_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
        .expect("built-in Zipformer model id should parse");
    let tts_id = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID)
        .expect("built-in Matcha model id should parse");
    let voice_id =
        tts::TtsVoiceId::parse("default").expect("built-in bilingual voice id should parse");
    let asr_catalog = asr::builtin_model_catalog();
    let tts_catalog = tts::builtin_model_catalog();
    let asr_total = asr_catalog
        .find(&asr_id)
        .expect("recommended ASR must exist in catalog")
        .artifact()
        .archive_bytes();
    let tts_total = tts_catalog
        .find(&tts_id)
        .expect("recommended TTS must exist in catalog")
        .package()
        .download_bytes();
    // completed = asr_total + tts_total / 2; package_total = asr_total + tts_total.
    let expected_percent = ((asr_total + tts_total / 2) * 100 / (asr_total + tts_total)) as u8;

    state.select_asr_model(Some(asr_id.clone()));
    state.model_manager_mut().mark_ready(&asr_id);
    state.select_tts_model_and_voice(Some(tts_id.clone()), Some(voice_id));
    state
        .tts_model_manager_mut()
        .begin_install(&tts_id, tts_total);
    state
        .tts_model_manager_mut()
        .update_progress(&tts_id, tts_total / 2, tts_total);

    assert_eq!(
        state.readiness(),
        VoiceEntryState::SettingUp {
            percent: expected_percent,
        },
        "recommended package progress must retain completed ASR bytes while TTS downloads"
    );
    assert_ne!(
        state.readiness(),
        VoiceEntryState::SettingUp { percent: 50 },
        "TTS-only progress must not replace the full recommended package progress"
    );
}
